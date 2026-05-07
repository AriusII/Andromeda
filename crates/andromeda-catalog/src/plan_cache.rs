//! V0 plan-class and bounded plan-cache gate.
//!
//! **Status: MINIMAL RUNTIME GATE.** This module still does not compile SRPL,
//! inspect histograms, or perform cost-based optimization. It defines the
//! bounded identity inputs, validates opaque plan candidates, selects a
//! deterministic minimal candidate, records advisory scenario evidence, and
//! guards an in-memory bounded cache with DecisionTrace-style evidence.
//!
//! ## What this module pins down
//!
//! 1. **Bounded `PlanClass` variants.**  A procedure may declare *one* plan
//!    class.  The set of variants is intentionally small and closed so we
//!    cannot accumulate ad-hoc shape buckets.  Adding a new variant is a
//!    doctrine change.
//! 2. **`CardinalityBucket`** is a bounded coarsening of input row counts.
//!    Bucket boundaries are baked in so the same input always maps to the
//!    same bucket on any node.
//! 3. **`PlanShapeFingerprint`** is a 32-byte SHA-256 digest over the
//!    procedure's bound parameter / type / cardinality evidence.  It is
//!    computed only from the inputs callers explicitly add; the scaffold
//!    never inspects values at runtime.
//! 4. **`PlanCacheKey`** is the full identity tuple a plan cache must key
//!    on.  Equality and hashing are derived from every version field, so
//!    bumping `StatsVersion`, `PolicyVersion`, `CatalogVersion`, or
//!    `ContractHash` automatically separates cache slots without any
//!    additional invalidation logic.
//! 5. **Minimal runtime gate.**  The cache stores only opaque plan ids and
//!    plan digests. It never stores executable SRPL state, never consults the
//!    statistics modules directly, and never treats scenario evidence as
//!    authoritative. Every hit, miss, insert, eviction, and selection emits a
//!    trace containing the key digest plus the bounded inputs that justified
//!    reuse, miss, or rejection. Trace-producing operations fail closed when
//!    the supplied [`TraceId`] is zero so optimizer decisions cannot become
//!    uncorrelated after the fact.
//!
//! ## What this module deliberately does NOT do
//!
//! - It does not compile a plan.
//! - It does not run a cost model.
//! - It does not compile SRPL.
//! - It does not consult statistics or histograms.
//! - It does not store executable plan state: cache entries carry opaque plan
//!   ids and plan digests only.
//! - It does not interpret SQL; the project remains Procedure-only.
//!
//! Any later optimizer crate can depend on these types without re-deriving the
//! key shape or weakening invalidation, ensuring decision traces remain
//! reproducible across catalog / stats / policy upgrades.
//!
//! ## Plan Cache Invalidation Policy
//!
//! The runtime cache follows these invalidation rules **strictly**: any
//! change to any component listed below must either create a new key entry or
//! reject reuse of an old entry. **Silent reuse is forbidden.**
//!
//! | Component | Invalidation Trigger | Example / Consequence |
//! | --- | --- | --- |
//! | `ProcedureId` | Procedure identity changes | Different procedure, never share cached plan |
//! | `ContractHash` | Procedure contract is altered (ALTER PROCEDURE) | Signature, result schema, or policy change invalidates all old keys |
//! | `CatalogVersion` | Catalog schema evolves | New catalog version = new key entry; old plans not reused |
//! | `StatsVersion` | Statistics histogram or correlation evidence is updated | New stats version = new key entry; old cardinality assumptions void |
//! | `PolicyVersion` | Policy surface mutates | New transaction policy or permissions = new key; strict isolation |
//! | `PlanClass` | Specialization strategy changes | Singleton → ParameterShape: different key space; no cross-class reuse |
//! | `PlanShapeFingerprint` | Parameter shape or cardinality evidence changes | Different bound parameters = different fingerprint = different key |
//!
//! ### No-Silent-Drop Guarantee
//!
//! When any of these components changes:
//! 1. The resulting `PlanCacheKey` is NOT equal to the old key (derived `PartialEq, Eq`).
//! 2. The key's `digest()` is different (SHA-256 includes each component with a unique tag).
//! 3. A runtime cache lookup with the old key will **not** retrieve a plan from a slot
//!    that was written with the new key.
//! 4. A runtime cache insertion with a new key will allocate a separate slot.
//!
//! This is enforced purely through Rust's type system and Eq/Hash derivation;
//! no explicit eviction logic is required as long as the cache keys on the
//! `PlanCacheKey` type itself.
//!
//! ### Evidence for Traces
//!
//! Every cache operation emits a [`DecisionTrace`] containing:
//! - The key's digest (privacy-safe 32-byte hash)
//! - The procedure ID, catalog version, stats version, policy version
//! - The plan class and shape fingerprint digest
//! - The operation type (hit, miss, insert, evict, invalidate, or reject)
//! - The bounded reason code (if rejected)

use andromeda_core::{CatalogVersion, ContractHash, EngineTimestamp, ProcedureId};
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};

use crate::contracts::{PolicyVersion, ProcedureContractBinding, StatsVersion};
use crate::digest::{Sha256, digest_prefix_hex};
use crate::scenario_evidence::{
    ScenarioEvidence, ScenarioEvidenceAdvisoryUse, ScenarioEvidenceError,
};

/// Bounded plan-class taxonomy.
///
/// The variants are closed.  Adding a variant is a doctrine change because
/// it expands what counts as a legitimate reason to maintain multiple plans
/// for a single procedure.  Each variant carries an `as_tag` byte that is
/// folded into the cache key so two plan classes never collide even when
/// their other key inputs are identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PlanClass {
    /// One plan per `(contract, catalog, stats, policy)` tuple.  No shape
    /// specialization at all.  This is the safe default.
    Singleton,
    /// Plans may diverge on bound parameter shape (types, nullability,
    /// structured arity).  The shape fingerprint participates in the key.
    ParameterShape,
    /// Plans may diverge on input cardinality bucket only.  The fingerprint
    /// must be derived from `CardinalityBucket` evidence alone.
    Cardinality,
    /// Plans may diverge on a combined parameter-shape and cardinality
    /// fingerprint.  Strictest specialization the scaffold permits.
    StatsAdaptive,
}

impl PlanClass {
    /// Stable tag byte folded into the plan-cache key digest.
    ///
    /// These byte values are part of the on-the-wire identity of a plan and
    /// must never be reordered.  Adding a variant must append a new tag.
    pub const fn as_tag(self) -> u8 {
        match self {
            PlanClass::Singleton => 0x01,
            PlanClass::ParameterShape => 0x02,
            PlanClass::Cardinality => 0x03,
            PlanClass::StatsAdaptive => 0x04,
        }
    }

    /// Total number of variants.  Asserted by tests so accidental growth is
    /// flagged immediately.
    pub const VARIANT_COUNT: usize = 4;

    /// Whether the plan class permits a non-empty
    /// [`PlanShapeFingerprint`] in the cache key.  `Singleton` rejects any
    /// shape input so plan reuse cannot be silently widened.
    pub const fn allows_shape_fingerprint(self) -> bool {
        !matches!(self, PlanClass::Singleton)
    }
}

/// Bounded cardinality bucket used to coarsen input row counts before they
/// influence a plan-cache key.  Boundaries are intentionally coarse so
/// statistical drift inside a bucket cannot churn the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CardinalityBucket {
    /// Exactly 0 input rows.
    Zero,
    /// 1 input row.
    One,
    /// 2..=64 input rows.
    Small,
    /// 65..=4096 input rows.
    Medium,
    /// 4097..=262_144 input rows.
    Large,
    /// > 262_144 input rows.
    Bulk,
}

impl CardinalityBucket {
    pub const VARIANT_COUNT: usize = 6;

    /// Map a row count to its bounded bucket.  Pure function: the same input
    /// always yields the same bucket on every node.
    pub const fn classify(row_count: u64) -> Self {
        if row_count == 0 {
            CardinalityBucket::Zero
        } else if row_count == 1 {
            CardinalityBucket::One
        } else if row_count <= 64 {
            CardinalityBucket::Small
        } else if row_count <= 4_096 {
            CardinalityBucket::Medium
        } else if row_count <= 262_144 {
            CardinalityBucket::Large
        } else {
            CardinalityBucket::Bulk
        }
    }

    /// Stable tag byte folded into the shape fingerprint.
    pub const fn as_tag(self) -> u8 {
        match self {
            CardinalityBucket::Zero => 0x10,
            CardinalityBucket::One => 0x11,
            CardinalityBucket::Small => 0x12,
            CardinalityBucket::Medium => 0x13,
            CardinalityBucket::Large => 0x14,
            CardinalityBucket::Bulk => 0x15,
        }
    }
}

/// 32-byte fingerprint over the bound shape evidence for a procedure call.
///
/// The fingerprint is purely derived from inputs the caller explicitly
/// supplies through [`PlanShapeFingerprintBuilder`]; the scaffold does not
/// inspect runtime values, plans, or histograms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlanShapeFingerprint([u8; Self::LEN]);

impl PlanShapeFingerprint {
    pub const LEN: usize = 32;

    /// The "no shape evidence" fingerprint, used by `PlanClass::Singleton`.
    pub const fn empty() -> Self {
        Self([0; Self::LEN])
    }

    pub const fn from_bytes(bytes: [u8; Self::LEN]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(self) -> [u8; Self::LEN] {
        self.0
    }

    pub fn is_empty(self) -> bool {
        self.0.iter().all(|byte| *byte == 0)
    }
}

impl Default for PlanShapeFingerprint {
    fn default() -> Self {
        Self::empty()
    }
}

/// Domain tag absorbed at the start of every shape-fingerprint hash.
const PLAN_SHAPE_DOMAIN: &[u8] = b"andromeda.plan_cache.shape.v0";

/// Domain tag absorbed at the start of every plan-cache key digest.
const PLAN_CACHE_DOMAIN: &[u8] = b"andromeda.plan_cache.key.v0";

/// Streaming builder for [`PlanShapeFingerprint`].
///
/// Callers must add evidence in the order the optimizer will later consume
/// it.  The builder is intentionally narrow: only typed parameter
/// descriptors, structured arities, and cardinality buckets are accepted.
/// No free-form bytes, no SQL text, no runtime values.
#[derive(Clone)]
pub struct PlanShapeFingerprintBuilder {
    hasher: Sha256,
    parameter_count: u32,
    cardinality_count: u32,
}

impl PlanShapeFingerprintBuilder {
    pub fn new() -> Self {
        let mut hasher = Sha256::new();
        hasher.update(PLAN_SHAPE_DOMAIN);
        Self {
            hasher,
            parameter_count: 0,
            cardinality_count: 0,
        }
    }

    /// Absorb a typed parameter slot.  `type_tag` is the catalog's stable
    /// type-tag byte; `nullable` is whether the bound value may be null;
    /// `structured_arity` is 0 for scalar parameters and the structured
    /// object arity otherwise.
    pub fn push_parameter(mut self, type_tag: u8, nullable: bool, structured_arity: u16) -> Self {
        self.hasher.update(&[0xA0]);
        self.hasher.update(&[type_tag]);
        self.hasher.update(&[u8::from(nullable)]);
        self.hasher.update(&structured_arity.to_le_bytes());
        self.parameter_count = self.parameter_count.saturating_add(1);
        self
    }

    /// Absorb a cardinality bucket for a named input set (e.g. a relation
    /// parameter).  `slot_index` orders the inputs deterministically.
    pub fn push_cardinality(mut self, slot_index: u16, bucket: CardinalityBucket) -> Self {
        self.hasher.update(&[0xA1]);
        self.hasher.update(&slot_index.to_le_bytes());
        self.hasher.update(&[bucket.as_tag()]);
        self.cardinality_count = self.cardinality_count.saturating_add(1);
        self
    }

    /// Finalize the fingerprint.  Empty builders still yield a stable,
    /// non-default fingerprint distinct from [`PlanShapeFingerprint::empty`].
    pub fn finish(mut self) -> PlanShapeFingerprint {
        self.hasher.update(&self.parameter_count.to_le_bytes());
        self.hasher.update(&self.cardinality_count.to_le_bytes());
        PlanShapeFingerprint::from_bytes(self.hasher.finalize())
    }
}

impl Default for PlanShapeFingerprintBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Full identity tuple a plan cache must key on.
///
/// All version-bearing fields participate in equality and hashing, so
/// advancing any of them automatically separates cache slots.  The
/// `plan_class` field guarantees that two callers using different plan
/// classes against the same procedure can never alias each other.
/// `ContractHash + CatalogVersion + StatsVersion` is the optimizer binding
/// triplet for an active plan; all three must match before scenario evidence
/// or an in-memory entry can be considered reusable.
///
/// This type does *not* hold a plan.  It is purely the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlanCacheKey {
    pub procedure_id: ProcedureId,
    pub contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
    pub stats_version: StatsVersion,
    pub policy_version: PolicyVersion,
    pub plan_class: PlanClass,
    pub shape_fingerprint: PlanShapeFingerprint,
}

impl PlanCacheKey {
    /// Build a `PlanCacheKey` from a contract binding plus a plan class and
    /// shape fingerprint.
    ///
    /// Returns an error when the inputs violate the bounded
    /// invariants:
    /// - the source [`ProcedureContractBinding`] must be fully non-zero,
    /// - `PlanClass::Singleton` requires the empty shape fingerprint.
    /// - Every other `PlanClass` requires a non-empty fingerprint.
    pub fn build(
        binding: &ProcedureContractBinding,
        plan_class: PlanClass,
        shape_fingerprint: PlanShapeFingerprint,
    ) -> Result<Self, PlanCacheKeyError> {
        if binding.procedure_id.get() == 0 {
            return Err(PlanCacheKeyError::ProcedureIdZero);
        }
        if binding.catalog_version.get() == 0 {
            return Err(PlanCacheKeyError::CatalogVersionZero);
        }
        if binding.contract_hash.is_zero() {
            return Err(PlanCacheKeyError::ContractHashZero);
        }
        if binding.stats_version.get() == 0 {
            return Err(PlanCacheKeyError::StatsVersionZero);
        }
        if binding.policy_version.is_zero() {
            return Err(PlanCacheKeyError::PolicyVersionZero);
        }
        match (plan_class, shape_fingerprint.is_empty()) {
            (PlanClass::Singleton, false) => {
                Err(PlanCacheKeyError::SingletonRejectsShapeFingerprint)
            }
            (other, true) if other != PlanClass::Singleton => {
                Err(PlanCacheKeyError::ShapedPlanClassRequiresFingerprint)
            }
            _ => Ok(Self {
                procedure_id: binding.procedure_id,
                contract_hash: binding.contract_hash,
                catalog_version: binding.catalog_version,
                stats_version: binding.stats_version,
                policy_version: binding.policy_version,
                plan_class,
                shape_fingerprint,
            }),
        }
    }

    /// Compute a deterministic 32-byte digest over the entire key.  Useful
    /// for trace records and observability without leaking individual
    /// fields.  The digest is stable across nodes because every input is
    /// absorbed with explicit byte tags and little-endian widths.
    pub fn digest(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(PLAN_CACHE_DOMAIN);
        hasher.update(&[0xC0]);
        hasher.update(&self.procedure_id.get().to_le_bytes());
        hasher.update(&[0xC1]);
        hasher.update(&self.contract_hash.as_bytes());
        hasher.update(&[0xC2]);
        hasher.update(&self.catalog_version.get().to_le_bytes());
        hasher.update(&[0xC3]);
        hasher.update(&self.stats_version.get().to_le_bytes());
        hasher.update(&[0xC4]);
        hasher.update(&self.policy_version.as_bytes());
        hasher.update(&[0xC5]);
        hasher.update(&[self.plan_class.as_tag()]);
        hasher.update(&[0xC6]);
        hasher.update(&self.shape_fingerprint.as_bytes());
        hasher.finalize()
    }
}

/// Errors raised while constructing a [`PlanCacheKey`].  The variants are
/// closed and intentionally narrow so a caller cannot bypass the bounded
/// `PlanClass` invariants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanCacheKeyError {
    /// Procedure id zero is reserved and cannot identify a plan owner.
    ProcedureIdZero,
    /// Catalog version zero is reserved and cannot participate in invalidation.
    CatalogVersionZero,
    /// Contract hash zero is reserved and cannot prove a bound contract.
    ContractHashZero,
    /// Statistics version zero is reserved for "no stats bound yet".
    StatsVersionZero,
    /// Policy version zero is reserved and cannot prove policy compatibility.
    PolicyVersionZero,
    /// `PlanClass::Singleton` was supplied with a non-empty fingerprint.
    SingletonRejectsShapeFingerprint,
    /// A non-singleton `PlanClass` was supplied with the empty fingerprint.
    ShapedPlanClassRequiresFingerprint,
}

impl core::fmt::Display for PlanCacheKeyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PlanCacheKeyError::ProcedureIdZero => {
                f.write_str("plan-cache key requires a non-zero ProcedureId")
            }
            PlanCacheKeyError::CatalogVersionZero => {
                f.write_str("plan-cache key requires a non-zero CatalogVersion")
            }
            PlanCacheKeyError::ContractHashZero => {
                f.write_str("plan-cache key requires a non-zero ContractHash")
            }
            PlanCacheKeyError::StatsVersionZero => {
                f.write_str("plan-cache key requires a non-zero StatsVersion")
            }
            PlanCacheKeyError::PolicyVersionZero => {
                f.write_str("plan-cache key requires a non-zero PolicyVersion")
            }
            PlanCacheKeyError::SingletonRejectsShapeFingerprint => {
                f.write_str("PlanClass::Singleton must use the empty PlanShapeFingerprint")
            }
            PlanCacheKeyError::ShapedPlanClassRequiresFingerprint => {
                f.write_str("non-Singleton PlanClass requires a non-empty PlanShapeFingerprint")
            }
        }
    }
}

/// Maximum number of plan candidates that the minimal selector may consider.
///
/// This keeps the V0 gate deterministic and prevents accidental unbounded
/// plan-class explosion before a dedicated optimizer crate exists.
pub const PLAN_SELECTION_MAX_CANDIDATES: usize = 8;

/// Maximum number of advisory [`ScenarioEvidence`] records consumed by one
/// selection decision.
pub const PLAN_SELECTION_MAX_SCENARIO_EVIDENCE: usize = 8;

/// Maximum number of entries in the bounded in-memory PlanCache gate.
pub const PLAN_CACHE_MAX_ENTRIES: usize = 64;

/// Stable opaque id for a candidate physical plan.
///
/// The catalog gate does not own executable plan state.  This id is an
/// external handle supplied by a compiler or future optimizer layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlanCandidateId(u64);

impl PlanCandidateId {
    /// Construct a non-zero candidate id.
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Deterministic static candidate rank.
///
/// Lower values are preferred.  The rank is intentionally separate from
/// [`ScenarioEvidence`]: benchmark evidence can be recorded and summarized,
/// but it cannot become the sole authority that selects a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlanCandidateRank(u16);

impl PlanCandidateRank {
    pub const MAX_RAW: u16 = 1_000;

    pub const fn from_permille(value: u16) -> Result<Self, PlanSelectionError> {
        if value > Self::MAX_RAW {
            Err(PlanSelectionError::CandidateRankOutOfRange)
        } else {
            Ok(Self(value))
        }
    }

    pub const fn permille(self) -> u16 {
        self.0
    }
}

/// Opaque candidate considered by the minimal plan selector.
///
/// The candidate carries only a class, a static rank, and a digest over the
/// external plan representation.  No SRPL body, plan text, SQL text, or
/// executable state enters the catalog cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanCandidate {
    plan_id: PlanCandidateId,
    plan_class: PlanClass,
    static_rank: PlanCandidateRank,
    plan_digest: [u8; 32],
}

impl PlanCandidate {
    pub fn new(
        plan_id: PlanCandidateId,
        plan_class: PlanClass,
        static_rank: PlanCandidateRank,
        plan_digest: [u8; 32],
    ) -> Result<Self, PlanSelectionError> {
        if plan_digest == [0; 32] {
            return Err(PlanSelectionError::PlanDigestZero);
        }
        Ok(Self {
            plan_id,
            plan_class,
            static_rank,
            plan_digest,
        })
    }

    pub const fn plan_id(self) -> PlanCandidateId {
        self.plan_id
    }

    pub const fn plan_class(self) -> PlanClass {
        self.plan_class
    }

    pub const fn static_rank(self) -> PlanCandidateRank {
        self.static_rank
    }

    pub const fn plan_digest(self) -> [u8; 32] {
        self.plan_digest
    }
}

pub const ADVISORY_EVIDENCE_STATUS_COUNT: usize = 11;

/// Closed status for each ScenarioEvidence record evaluated by a plan
/// selection decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvisoryEvidenceStatus {
    /// Evidence matches the key and is fresh, but remains advisory-only.
    AcceptedAdvisory,
    /// Evidence unexpectedly reported itself as authoritative.
    AuthoritativeRejected,
    /// Evidence was issued in the future relative to the supplied clock.
    NotYetValid,
    /// Evidence expired before the supplied clock.
    Expired,
    /// Evidence targets a different Procedure.
    ProcedureIdMismatch,
    /// Evidence targets a different catalog publication.
    CatalogVersionMismatch,
    /// Evidence targets a different statistics publication.
    StatsVersionMismatch,
    /// Evidence does not bind the ContractHash, so it cannot be consumed for
    /// a contract-keyed plan decision.
    ContractHashMissing,
    /// Evidence binds a different ContractHash.
    ContractHashMismatch,
    /// Evidence does not bind the PlanClass.
    PlanClassMissing,
    /// Evidence binds a different PlanClass.
    PlanClassMismatch,
}

impl AdvisoryEvidenceStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            AdvisoryEvidenceStatus::AcceptedAdvisory => "accepted-advisory",
            AdvisoryEvidenceStatus::AuthoritativeRejected => "authoritative-rejected",
            AdvisoryEvidenceStatus::NotYetValid => "not-yet-valid",
            AdvisoryEvidenceStatus::Expired => "expired",
            AdvisoryEvidenceStatus::ProcedureIdMismatch => "procedure-id-mismatch",
            AdvisoryEvidenceStatus::CatalogVersionMismatch => "catalog-version-mismatch",
            AdvisoryEvidenceStatus::StatsVersionMismatch => "stats-version-mismatch",
            AdvisoryEvidenceStatus::ContractHashMissing => "contract-hash-missing",
            AdvisoryEvidenceStatus::ContractHashMismatch => "contract-hash-mismatch",
            AdvisoryEvidenceStatus::PlanClassMissing => "plan-class-missing",
            AdvisoryEvidenceStatus::PlanClassMismatch => "plan-class-mismatch",
        }
    }

    pub const fn as_index(self) -> usize {
        match self {
            AdvisoryEvidenceStatus::AcceptedAdvisory => 0,
            AdvisoryEvidenceStatus::AuthoritativeRejected => 1,
            AdvisoryEvidenceStatus::NotYetValid => 2,
            AdvisoryEvidenceStatus::Expired => 3,
            AdvisoryEvidenceStatus::ProcedureIdMismatch => 4,
            AdvisoryEvidenceStatus::CatalogVersionMismatch => 5,
            AdvisoryEvidenceStatus::StatsVersionMismatch => 6,
            AdvisoryEvidenceStatus::ContractHashMissing => 7,
            AdvisoryEvidenceStatus::ContractHashMismatch => 8,
            AdvisoryEvidenceStatus::PlanClassMissing => 9,
            AdvisoryEvidenceStatus::PlanClassMismatch => 10,
        }
    }
}

const ADVISORY_EVIDENCE_STATUSES: [AdvisoryEvidenceStatus; ADVISORY_EVIDENCE_STATUS_COUNT] = [
    AdvisoryEvidenceStatus::AcceptedAdvisory,
    AdvisoryEvidenceStatus::AuthoritativeRejected,
    AdvisoryEvidenceStatus::NotYetValid,
    AdvisoryEvidenceStatus::Expired,
    AdvisoryEvidenceStatus::ProcedureIdMismatch,
    AdvisoryEvidenceStatus::CatalogVersionMismatch,
    AdvisoryEvidenceStatus::StatsVersionMismatch,
    AdvisoryEvidenceStatus::ContractHashMissing,
    AdvisoryEvidenceStatus::ContractHashMismatch,
    AdvisoryEvidenceStatus::PlanClassMissing,
    AdvisoryEvidenceStatus::PlanClassMismatch,
];

fn classify_validated_advisory_evidence_for_key(
    key: &PlanCacheKey,
    evidence: &ScenarioEvidenceAdvisoryUse,
) -> AdvisoryEvidenceStatus {
    if evidence.is_authoritative() || evidence.can_select_plan_alone() {
        return AdvisoryEvidenceStatus::AuthoritativeRejected;
    }

    let target = evidence.target();
    if target.procedure_id != key.procedure_id {
        return AdvisoryEvidenceStatus::ProcedureIdMismatch;
    }
    if target.catalog_version != key.catalog_version {
        return AdvisoryEvidenceStatus::CatalogVersionMismatch;
    }
    if target.stats_version != key.stats_version {
        return AdvisoryEvidenceStatus::StatsVersionMismatch;
    }
    match target.contract_hash {
        Some(hash) if hash == key.contract_hash => {}
        Some(_) => return AdvisoryEvidenceStatus::ContractHashMismatch,
        None => return AdvisoryEvidenceStatus::ContractHashMissing,
    }
    match target.plan_class {
        Some(plan_class) if plan_class == key.plan_class => {}
        Some(_) => return AdvisoryEvidenceStatus::PlanClassMismatch,
        None => return AdvisoryEvidenceStatus::PlanClassMissing,
    }
    AdvisoryEvidenceStatus::AcceptedAdvisory
}

fn classify_advisory_evidence_for_key_with_token(
    key: &PlanCacheKey,
    evidence: &ScenarioEvidence,
    now: EngineTimestamp,
) -> (AdvisoryEvidenceStatus, Option<ScenarioEvidenceAdvisoryUse>) {
    if evidence.is_authoritative() {
        return (AdvisoryEvidenceStatus::AuthoritativeRejected, None);
    }
    let advisory = match evidence.advisory_use_at(now) {
        Ok(advisory) => advisory,
        Err(ScenarioEvidenceError::NotYetValid) => {
            return (AdvisoryEvidenceStatus::NotYetValid, None);
        }
        Err(ScenarioEvidenceError::Expired) => return (AdvisoryEvidenceStatus::Expired, None),
        Err(_) => return (AdvisoryEvidenceStatus::AuthoritativeRejected, None),
    };
    let status = classify_validated_advisory_evidence_for_key(key, &advisory);
    let advisory = match status {
        AdvisoryEvidenceStatus::AcceptedAdvisory => Some(advisory),
        _ => None,
    };
    (status, advisory)
}

/// Classify one ScenarioEvidence record against a complete plan-cache key.
///
/// This function deliberately requires evidence to match the key's
/// `ProcedureId`, `CatalogVersion`, `StatsVersion`, `ContractHash`, and
/// `PlanClass`.  Evidence carries no policy version, so a cache hit remains
/// governed by the full [`PlanCacheKey`] rather than by benchmark output.
pub fn classify_advisory_evidence_for_key(
    key: &PlanCacheKey,
    evidence: &ScenarioEvidence,
    now: EngineTimestamp,
) -> AdvisoryEvidenceStatus {
    classify_advisory_evidence_for_key_with_token(key, evidence, now).0
}

/// Bounded summary of advisory evidence considered for a plan decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdvisoryEvidenceSummary {
    supplied_count: u8,
    accepted_count: u8,
    rejected_count: u8,
    status_counts: [u8; ADVISORY_EVIDENCE_STATUS_COUNT],
    best_score_permille: Option<u16>,
    best_confidence_permille: Option<u16>,
    best_evidence_digest: Option<[u8; 32]>,
}

impl AdvisoryEvidenceSummary {
    pub const fn empty() -> Self {
        Self {
            supplied_count: 0,
            accepted_count: 0,
            rejected_count: 0,
            status_counts: [0; ADVISORY_EVIDENCE_STATUS_COUNT],
            best_score_permille: None,
            best_confidence_permille: None,
            best_evidence_digest: None,
        }
    }

    pub const fn has_accepted_advisory_evidence(self) -> bool {
        self.accepted_count > 0
    }

    pub const fn supplied_count(self) -> u8 {
        self.supplied_count
    }

    pub const fn accepted_count(self) -> u8 {
        self.accepted_count
    }

    pub const fn rejected_count(self) -> u8 {
        self.rejected_count
    }

    pub const fn best_score_permille(self) -> Option<u16> {
        self.best_score_permille
    }

    pub const fn best_confidence_permille(self) -> Option<u16> {
        self.best_confidence_permille
    }

    pub const fn best_evidence_digest(self) -> Option<[u8; 32]> {
        self.best_evidence_digest
    }

    pub fn status_count(&self, status: AdvisoryEvidenceStatus) -> u8 {
        self.status_counts[status.as_index()]
    }
}

fn summarize_advisory_evidence(
    key: &PlanCacheKey,
    evidence: &[ScenarioEvidence],
    now: EngineTimestamp,
) -> Result<AdvisoryEvidenceSummary, PlanSelectionError> {
    if evidence.len() > PLAN_SELECTION_MAX_SCENARIO_EVIDENCE {
        return Err(PlanSelectionError::TooManyScenarioEvidence);
    }

    let mut summary = AdvisoryEvidenceSummary {
        supplied_count: evidence.len() as u8,
        ..AdvisoryEvidenceSummary::empty()
    };

    for item in evidence {
        let (status, advisory) = classify_advisory_evidence_for_key_with_token(key, item, now);
        summary.status_counts[status.as_index()] =
            summary.status_counts[status.as_index()].saturating_add(1);

        match status {
            AdvisoryEvidenceStatus::AcceptedAdvisory => {
                let Some(advisory) = advisory else {
                    summary.rejected_count = summary.rejected_count.saturating_add(1);
                    continue;
                };
                summary.accepted_count = summary.accepted_count.saturating_add(1);
                let score = advisory.score().permille();
                let confidence = advisory.confidence().permille();
                let digest = advisory.digest();

                let replace_best = match (
                    summary.best_score_permille,
                    summary.best_confidence_permille,
                    summary.best_evidence_digest,
                ) {
                    (None, _, _) => true,
                    (Some(best_score), Some(best_confidence), Some(best_digest)) => {
                        (score, confidence, digest) > (best_score, best_confidence, best_digest)
                    }
                    _ => true,
                };

                if replace_best {
                    summary.best_score_permille = Some(score);
                    summary.best_confidence_permille = Some(confidence);
                    summary.best_evidence_digest = Some(digest);
                }
            }
            _ => {
                summary.rejected_count = summary.rejected_count.saturating_add(1);
            }
        }
    }

    Ok(summary)
}

fn advisory_status_counts_reason(summary: &AdvisoryEvidenceSummary) -> String {
    if summary.supplied_count == 0 {
        return "none".to_string();
    }

    let mut out = String::new();
    for status in ADVISORY_EVIDENCE_STATUSES {
        let count = summary.status_count(status);
        if count == 0 {
            continue;
        }
        if !out.is_empty() {
            out.push(',');
        }
        out.push_str(status.as_str());
        out.push(':');
        out.push_str(&count.to_string());
    }
    out
}

/// Closed outcome for a plan-cache or plan-selection decision trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanDecisionOutcome {
    Selected,
    CacheHit,
    CacheMiss,
    CacheInsert,
    CacheEvict,
    Rejected,
}

impl PlanDecisionOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            PlanDecisionOutcome::Selected => "selected",
            PlanDecisionOutcome::CacheHit => "cache-hit",
            PlanDecisionOutcome::CacheMiss => "cache-miss",
            PlanDecisionOutcome::CacheInsert => "cache-insert",
            PlanDecisionOutcome::CacheEvict => "cache-evict",
            PlanDecisionOutcome::Rejected => "rejected",
        }
    }
}

/// Closed reason codes for the minimal PlanCache gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanDecisionReasonCode {
    MinimalRankSelected,
    ExactKeyHit,
    ExactKeyMiss,
    Inserted,
    CapacityEvictedOldest,
}

/// Bounded explanation for an exact-key PlanCache miss.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanCacheMissReason {
    CacheEmpty,
    NoProcedureEntry,
    ContractHashMismatch,
    CatalogVersionMismatch,
    StatsVersionMismatch,
    PolicyVersionMismatch,
    PlanClassMismatch,
    ShapeFingerprintMismatch,
}

impl PlanCacheMissReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            PlanCacheMissReason::CacheEmpty => "cache-empty",
            PlanCacheMissReason::NoProcedureEntry => "no-procedure-entry",
            PlanCacheMissReason::ContractHashMismatch => "contract-hash-mismatch",
            PlanCacheMissReason::CatalogVersionMismatch => "catalog-version-mismatch",
            PlanCacheMissReason::StatsVersionMismatch => "stats-version-mismatch",
            PlanCacheMissReason::PolicyVersionMismatch => "policy-version-mismatch",
            PlanCacheMissReason::PlanClassMismatch => "plan-class-mismatch",
            PlanCacheMissReason::ShapeFingerprintMismatch => "shape-fingerprint-mismatch",
        }
    }
}

impl PlanDecisionReasonCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            PlanDecisionReasonCode::MinimalRankSelected => "minimal-rank-selected",
            PlanDecisionReasonCode::ExactKeyHit => "exact-key-hit",
            PlanDecisionReasonCode::ExactKeyMiss => "exact-key-miss",
            PlanDecisionReasonCode::Inserted => "inserted",
            PlanDecisionReasonCode::CapacityEvictedOldest => "capacity-evicted-oldest",
        }
    }
}

/// DecisionTrace-style evidence emitted by the minimal selector and cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanDecisionEvidence {
    trace_id: TraceId,
    key: PlanCacheKey,
    key_digest: [u8; 32],
    outcome: PlanDecisionOutcome,
    reason_code: PlanDecisionReasonCode,
    selected_plan_id: Option<PlanCandidateId>,
    candidate_count: u8,
    matching_candidate_count: u8,
    advisory_evidence: AdvisoryEvidenceSummary,
    cache_miss_reason: Option<PlanCacheMissReason>,
}

impl PlanDecisionEvidence {
    #[allow(
        clippy::too_many_arguments,
        reason = "Trace evidence keeps every plan decision input explicit."
    )]
    fn new(
        trace_id: TraceId,
        key: PlanCacheKey,
        outcome: PlanDecisionOutcome,
        reason_code: PlanDecisionReasonCode,
        selected_plan_id: Option<PlanCandidateId>,
        candidate_count: u8,
        matching_candidate_count: u8,
        advisory_evidence: AdvisoryEvidenceSummary,
    ) -> Self {
        Self {
            trace_id,
            key,
            key_digest: key.digest(),
            outcome,
            reason_code,
            selected_plan_id,
            candidate_count,
            matching_candidate_count,
            advisory_evidence,
            cache_miss_reason: None,
        }
    }

    fn with_cache_miss_reason(mut self, reason: Option<PlanCacheMissReason>) -> Self {
        self.cache_miss_reason = reason;
        self
    }

    pub const fn trace_id(&self) -> TraceId {
        self.trace_id
    }

    pub const fn key(&self) -> PlanCacheKey {
        self.key
    }

    pub const fn key_digest(&self) -> [u8; 32] {
        self.key_digest
    }

    pub const fn outcome(&self) -> PlanDecisionOutcome {
        self.outcome
    }

    pub const fn selected_plan_id(&self) -> Option<PlanCandidateId> {
        self.selected_plan_id
    }

    pub const fn candidate_count(&self) -> u8 {
        self.candidate_count
    }

    pub const fn matching_candidate_count(&self) -> u8 {
        self.matching_candidate_count
    }

    pub const fn advisory_evidence(&self) -> AdvisoryEvidenceSummary {
        self.advisory_evidence
    }

    pub const fn cache_miss_reason(&self) -> Option<PlanCacheMissReason> {
        self.cache_miss_reason
    }

    pub fn as_decision_trace(&self) -> DecisionTrace {
        DecisionTrace {
            trace_id: self.trace_id,
            decision: CriticalDecisionKind::PlanSelection,
            reason: self.reason(),
        }
    }

    pub fn reason(&self) -> String {
        let selected = self
            .selected_plan_id
            .map(|plan_id| plan_id.get().to_string())
            .unwrap_or_else(|| "none".to_string());
        let best_evidence_digest = self
            .advisory_evidence
            .best_evidence_digest
            .map(|digest| digest_prefix_hex(&digest))
            .unwrap_or_else(|| "none".to_string());
        let cache_miss_reason = self
            .cache_miss_reason
            .map(PlanCacheMissReason::as_str)
            .unwrap_or("none");

        format!(
            concat!(
                "plan-decision outcome={} reason={} key_digest={} ",
                "version_binding=ContractHash+CatalogVersion+StatsVersion ",
                "procedure_id={} contract_hash={} catalog_version={} stats_version={} policy_version={} ",
                "plan_class={:?} shape_digest={} selected_plan_id={} ",
                "candidates={} matching_candidates={} scenario_evidence_supplied={} ",
                "scenario_evidence_accepted_advisory={} scenario_evidence_rejected={} ",
                "scenario_evidence_statuses={} best_evidence_digest={} ",
                "cache_miss_reason={} advisory_only=true"
            ),
            self.outcome.as_str(),
            self.reason_code.as_str(),
            digest_prefix_hex(&self.key_digest),
            self.key.procedure_id.get(),
            digest_prefix_hex(&self.key.contract_hash.as_bytes()),
            self.key.catalog_version.get(),
            self.key.stats_version.get(),
            digest_prefix_hex(&self.key.policy_version.as_bytes()),
            self.key.plan_class,
            digest_prefix_hex(&self.key.shape_fingerprint.as_bytes()),
            selected,
            self.candidate_count,
            self.matching_candidate_count,
            self.advisory_evidence.supplied_count,
            self.advisory_evidence.accepted_count,
            self.advisory_evidence.rejected_count,
            advisory_status_counts_reason(&self.advisory_evidence),
            best_evidence_digest,
            cache_miss_reason,
        )
    }
}

/// Result of deterministic minimal plan selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanSelectionOutcome {
    key: PlanCacheKey,
    selected: PlanCandidate,
    trace: PlanDecisionEvidence,
}

impl PlanSelectionOutcome {
    pub const fn key(&self) -> PlanCacheKey {
        self.key
    }

    pub const fn selected(&self) -> PlanCandidate {
        self.selected
    }

    pub const fn trace(&self) -> &PlanDecisionEvidence {
        &self.trace
    }
}

/// Select the lowest-ranked candidate that matches the key's [`PlanClass`].
///
/// ScenarioEvidence is validated, summarized, and traced, but it never
/// overrides the bounded static rank.  Ties are broken by `PlanCandidateId`
/// for deterministic cross-node behavior.
pub fn select_minimal_plan(
    key: PlanCacheKey,
    candidates: &[PlanCandidate],
    scenario_evidence: &[ScenarioEvidence],
    now: EngineTimestamp,
    trace_id: TraceId,
) -> Result<PlanSelectionOutcome, PlanSelectionError> {
    if trace_id.is_zero() {
        return Err(PlanSelectionError::TraceIdZero);
    }
    if candidates.is_empty() {
        return Err(PlanSelectionError::NoCandidates);
    }
    if candidates.len() > PLAN_SELECTION_MAX_CANDIDATES {
        return Err(PlanSelectionError::TooManyCandidates);
    }

    let advisory_evidence = summarize_advisory_evidence(&key, scenario_evidence, now)?;
    let mut selected: Option<PlanCandidate> = None;
    let mut matching_candidate_count = 0u8;

    for candidate in candidates {
        if candidate.plan_class() != key.plan_class {
            continue;
        }
        matching_candidate_count = matching_candidate_count.saturating_add(1);
        let should_replace = match selected {
            None => true,
            Some(current) => {
                (candidate.static_rank(), candidate.plan_id())
                    < (current.static_rank(), current.plan_id())
            }
        };
        if should_replace {
            selected = Some(*candidate);
        }
    }

    let selected = selected.ok_or(PlanSelectionError::NoCandidateForPlanClass)?;
    let trace = PlanDecisionEvidence::new(
        trace_id,
        key,
        PlanDecisionOutcome::Selected,
        PlanDecisionReasonCode::MinimalRankSelected,
        Some(selected.plan_id()),
        candidates.len() as u8,
        matching_candidate_count,
        advisory_evidence,
    );

    Ok(PlanSelectionOutcome {
        key,
        selected,
        trace,
    })
}

/// Errors raised by the minimal plan selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanSelectionError {
    TraceIdZero,
    CandidateRankOutOfRange,
    PlanDigestZero,
    NoCandidates,
    TooManyCandidates,
    TooManyScenarioEvidence,
    NoCandidateForPlanClass,
}

impl core::fmt::Display for PlanSelectionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PlanSelectionError::TraceIdZero => {
                f.write_str("plan selection requires a non-zero TraceId")
            }
            PlanSelectionError::CandidateRankOutOfRange => {
                f.write_str("PlanCandidateRank raw value must be in 0..=1000")
            }
            PlanSelectionError::PlanDigestZero => {
                f.write_str("PlanCandidate.plan_digest must be non-zero")
            }
            PlanSelectionError::NoCandidates => {
                f.write_str("plan selection requires at least one candidate")
            }
            PlanSelectionError::TooManyCandidates => {
                f.write_str("plan selection candidate count exceeds the bounded maximum")
            }
            PlanSelectionError::TooManyScenarioEvidence => {
                f.write_str("scenario evidence count exceeds the bounded maximum")
            }
            PlanSelectionError::NoCandidateForPlanClass => {
                f.write_str("no candidate matched the requested PlanClass")
            }
        }
    }
}

/// Opaque entry stored by the bounded PlanCache gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanCacheEntry {
    key: PlanCacheKey,
    key_digest: [u8; 32],
    plan_id: PlanCandidateId,
    plan_digest: [u8; 32],
}

impl PlanCacheEntry {
    pub fn new(
        key: PlanCacheKey,
        plan_id: PlanCandidateId,
        plan_digest: [u8; 32],
    ) -> Result<Self, PlanCacheError> {
        if plan_digest == [0; 32] {
            return Err(PlanCacheError::PlanDigestZero);
        }
        Ok(Self {
            key,
            key_digest: key.digest(),
            plan_id,
            plan_digest,
        })
    }

    pub fn from_selection(selection: &PlanSelectionOutcome) -> Result<Self, PlanCacheError> {
        Self::new(
            selection.key(),
            selection.selected().plan_id(),
            selection.selected().plan_digest(),
        )
    }

    pub const fn key(self) -> PlanCacheKey {
        self.key
    }

    pub const fn key_digest(self) -> [u8; 32] {
        self.key_digest
    }

    pub const fn plan_id(self) -> PlanCandidateId {
        self.plan_id
    }

    pub const fn plan_digest(self) -> [u8; 32] {
        self.plan_digest
    }

    pub fn validates_for_key(&self, key: &PlanCacheKey) -> Result<(), PlanCacheError> {
        if self.key != *key {
            return Err(PlanCacheError::KeyMismatch);
        }
        if self.key_digest != key.digest() {
            return Err(PlanCacheError::KeyDigestMismatch);
        }
        if self.plan_digest == [0; 32] {
            return Err(PlanCacheError::PlanDigestZero);
        }
        Ok(())
    }
}

/// Bounded in-memory PlanCache gate.
///
/// The cache is deliberately small and deterministic.  It uses insertion
/// order eviction and exact [`PlanCacheKey`] equality only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedPlanCache {
    capacity: usize,
    entries: Vec<PlanCacheEntry>,
}

impl BoundedPlanCache {
    pub fn new(capacity: usize) -> Result<Self, PlanCacheError> {
        if capacity == 0 {
            return Err(PlanCacheError::CapacityZero);
        }
        if capacity > PLAN_CACHE_MAX_ENTRIES {
            return Err(PlanCacheError::CapacityTooLarge);
        }
        Ok(Self {
            capacity,
            entries: Vec::with_capacity(capacity),
        })
    }

    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn insert_selection(
        &mut self,
        selection: &PlanSelectionOutcome,
        trace_id: TraceId,
    ) -> Result<PlanCacheInsertReport, PlanCacheError> {
        self.insert_entry(PlanCacheEntry::from_selection(selection)?, trace_id)
    }

    pub fn insert_entry(
        &mut self,
        entry: PlanCacheEntry,
        trace_id: TraceId,
    ) -> Result<PlanCacheInsertReport, PlanCacheError> {
        if trace_id.is_zero() {
            return Err(PlanCacheError::TraceIdZero);
        }
        entry.validates_for_key(&entry.key)?;

        if let Some(position) = self
            .entries
            .iter()
            .position(|existing| existing.key == entry.key)
        {
            self.entries.remove(position);
        }

        let evicted = if self.entries.len() == self.capacity {
            Some(self.entries.remove(0))
        } else {
            None
        };
        self.entries.push(entry);

        let insert_trace = PlanDecisionEvidence::new(
            trace_id,
            entry.key,
            PlanDecisionOutcome::CacheInsert,
            PlanDecisionReasonCode::Inserted,
            Some(entry.plan_id),
            0,
            0,
            AdvisoryEvidenceSummary::empty(),
        );
        let eviction_trace = evicted.map(|evicted| {
            PlanDecisionEvidence::new(
                trace_id,
                evicted.key,
                PlanDecisionOutcome::CacheEvict,
                PlanDecisionReasonCode::CapacityEvictedOldest,
                Some(evicted.plan_id),
                0,
                0,
                AdvisoryEvidenceSummary::empty(),
            )
        });

        Ok(PlanCacheInsertReport {
            inserted: entry,
            evicted,
            insert_trace,
            eviction_trace,
        })
    }

    pub fn lookup(
        &self,
        key: PlanCacheKey,
        trace_id: TraceId,
    ) -> Result<PlanCacheLookupReport, PlanCacheError> {
        if trace_id.is_zero() {
            return Err(PlanCacheError::TraceIdZero);
        }
        let found = self
            .entries
            .iter()
            .copied()
            .find(|entry| entry.key == key)
            .and_then(|entry| match entry.validates_for_key(&key) {
                Ok(()) => Some(entry),
                Err(_) => None,
            });

        let (outcome, reason_code, selected_plan_id) = match found {
            Some(entry) => (
                PlanDecisionOutcome::CacheHit,
                PlanDecisionReasonCode::ExactKeyHit,
                Some(entry.plan_id),
            ),
            None => (
                PlanDecisionOutcome::CacheMiss,
                PlanDecisionReasonCode::ExactKeyMiss,
                None,
            ),
        };
        let cache_miss_reason = if found.is_some() {
            None
        } else {
            Some(self.classify_miss(&key))
        };
        let trace = PlanDecisionEvidence::new(
            trace_id,
            key,
            outcome,
            reason_code,
            selected_plan_id,
            0,
            0,
            AdvisoryEvidenceSummary::empty(),
        )
        .with_cache_miss_reason(cache_miss_reason);

        Ok(PlanCacheLookupReport {
            key,
            entry: found,
            trace,
        })
    }

    fn classify_miss(&self, key: &PlanCacheKey) -> PlanCacheMissReason {
        if self.entries.is_empty() {
            return PlanCacheMissReason::CacheEmpty;
        }

        let mut procedure_match = false;
        let mut contract_match = false;
        let mut catalog_match = false;
        let mut stats_match = false;
        let mut policy_match = false;
        let mut plan_class_match = false;

        for entry in &self.entries {
            if entry.key.procedure_id != key.procedure_id {
                continue;
            }
            procedure_match = true;
            if entry.key.contract_hash != key.contract_hash {
                continue;
            }
            contract_match = true;
            if entry.key.catalog_version != key.catalog_version {
                continue;
            }
            catalog_match = true;
            if entry.key.stats_version != key.stats_version {
                continue;
            }
            stats_match = true;
            if entry.key.policy_version != key.policy_version {
                continue;
            }
            policy_match = true;
            if entry.key.plan_class != key.plan_class {
                continue;
            }
            plan_class_match = true;
            if entry.key.shape_fingerprint != key.shape_fingerprint {
                continue;
            }
        }

        if !procedure_match {
            PlanCacheMissReason::NoProcedureEntry
        } else if !contract_match {
            PlanCacheMissReason::ContractHashMismatch
        } else if !catalog_match {
            PlanCacheMissReason::CatalogVersionMismatch
        } else if !stats_match {
            PlanCacheMissReason::StatsVersionMismatch
        } else if !policy_match {
            PlanCacheMissReason::PolicyVersionMismatch
        } else if !plan_class_match {
            PlanCacheMissReason::PlanClassMismatch
        } else {
            PlanCacheMissReason::ShapeFingerprintMismatch
        }
    }
}

/// Result of inserting into the bounded PlanCache gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanCacheInsertReport {
    inserted: PlanCacheEntry,
    evicted: Option<PlanCacheEntry>,
    insert_trace: PlanDecisionEvidence,
    eviction_trace: Option<PlanDecisionEvidence>,
}

impl PlanCacheInsertReport {
    pub const fn inserted(&self) -> PlanCacheEntry {
        self.inserted
    }

    pub const fn evicted(&self) -> Option<PlanCacheEntry> {
        self.evicted
    }

    pub const fn insert_trace(&self) -> &PlanDecisionEvidence {
        &self.insert_trace
    }

    pub fn eviction_trace(&self) -> Option<&PlanDecisionEvidence> {
        self.eviction_trace.as_ref()
    }
}

/// Result of an exact-key cache lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanCacheLookupReport {
    key: PlanCacheKey,
    entry: Option<PlanCacheEntry>,
    trace: PlanDecisionEvidence,
}

impl PlanCacheLookupReport {
    pub const fn key(&self) -> PlanCacheKey {
        self.key
    }

    pub const fn entry(&self) -> Option<PlanCacheEntry> {
        self.entry
    }

    pub const fn trace(&self) -> &PlanDecisionEvidence {
        &self.trace
    }

    pub const fn is_hit(&self) -> bool {
        self.entry.is_some()
    }
}

/// Errors raised by the bounded PlanCache gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanCacheError {
    TraceIdZero,
    CapacityZero,
    CapacityTooLarge,
    KeyMismatch,
    KeyDigestMismatch,
    PlanDigestZero,
}

impl core::fmt::Display for PlanCacheError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PlanCacheError::TraceIdZero => {
                f.write_str("PlanCache trace-producing operations require a non-zero TraceId")
            }
            PlanCacheError::CapacityZero => f.write_str("PlanCache capacity must be non-zero"),
            PlanCacheError::CapacityTooLarge => {
                f.write_str("PlanCache capacity exceeds the bounded maximum")
            }
            PlanCacheError::KeyMismatch => {
                f.write_str("PlanCache entry key does not match lookup key")
            }
            PlanCacheError::KeyDigestMismatch => {
                f.write_str("PlanCache entry key digest does not match lookup key digest")
            }
            PlanCacheError::PlanDigestZero => {
                f.write_str("PlanCache entry plan digest must be non-zero")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(
        procedure: u64,
        catalog: u64,
        contract_byte: u8,
        stats: u64,
        policy_byte: u8,
    ) -> ProcedureContractBinding {
        ProcedureContractBinding {
            procedure_id: ProcedureId::new(procedure),
            catalog_version: CatalogVersion::new(catalog),
            contract_hash: ContractHash::new([contract_byte; ContractHash::LEN]),
            stats_version: StatsVersion::new(stats),
            policy_version: PolicyVersion::new([policy_byte; PolicyVersion::LEN]),
        }
    }

    fn shaped_fingerprint() -> PlanShapeFingerprint {
        PlanShapeFingerprintBuilder::new()
            .push_parameter(0x01, false, 0)
            .push_parameter(0x02, true, 0)
            .push_cardinality(0, CardinalityBucket::classify(100))
            .finish()
    }

    #[test]
    fn plan_class_variant_count_is_bounded() {
        // Asserts the bounded set is exactly what doctrine permits.  Any
        // accidental new variant must update both this constant and the
        // documented taxonomy in the module preamble.
        assert_eq!(PlanClass::VARIANT_COUNT, 4);
        let tags = [
            PlanClass::Singleton.as_tag(),
            PlanClass::ParameterShape.as_tag(),
            PlanClass::Cardinality.as_tag(),
            PlanClass::StatsAdaptive.as_tag(),
        ];
        let mut sorted = tags;
        sorted.sort_unstable();
        assert!(
            sorted.windows(2).all(|pair| pair[0] != pair[1]),
            "PlanClass tags must be unique"
        );
    }

    #[test]
    fn cardinality_bucket_boundaries_are_stable() {
        assert_eq!(CardinalityBucket::VARIANT_COUNT, 6);
        assert_eq!(CardinalityBucket::classify(0), CardinalityBucket::Zero);
        assert_eq!(CardinalityBucket::classify(1), CardinalityBucket::One);
        assert_eq!(CardinalityBucket::classify(2), CardinalityBucket::Small);
        assert_eq!(CardinalityBucket::classify(64), CardinalityBucket::Small);
        assert_eq!(CardinalityBucket::classify(65), CardinalityBucket::Medium);
        assert_eq!(
            CardinalityBucket::classify(4_096),
            CardinalityBucket::Medium
        );
        assert_eq!(CardinalityBucket::classify(4_097), CardinalityBucket::Large);
        assert_eq!(
            CardinalityBucket::classify(262_144),
            CardinalityBucket::Large
        );
        assert_eq!(
            CardinalityBucket::classify(262_145),
            CardinalityBucket::Bulk
        );
    }

    #[test]
    fn plan_shape_fingerprint_is_deterministic() {
        let lhs = shaped_fingerprint();
        let rhs = shaped_fingerprint();
        assert_eq!(lhs, rhs);
        assert!(!lhs.is_empty());
        assert_ne!(lhs, PlanShapeFingerprint::empty());
    }

    #[test]
    fn plan_shape_fingerprint_separates_on_each_input() {
        let base = shaped_fingerprint();

        // Different parameter type tag.
        let other_type = PlanShapeFingerprintBuilder::new()
            .push_parameter(0x09, false, 0)
            .push_parameter(0x02, true, 0)
            .push_cardinality(0, CardinalityBucket::classify(100))
            .finish();
        assert_ne!(base, other_type);

        // Different nullability.
        let other_null = PlanShapeFingerprintBuilder::new()
            .push_parameter(0x01, true, 0)
            .push_parameter(0x02, true, 0)
            .push_cardinality(0, CardinalityBucket::classify(100))
            .finish();
        assert_ne!(base, other_null);

        // Different cardinality bucket.
        let other_card = PlanShapeFingerprintBuilder::new()
            .push_parameter(0x01, false, 0)
            .push_parameter(0x02, true, 0)
            .push_cardinality(0, CardinalityBucket::classify(10_000))
            .finish();
        assert_ne!(base, other_card);

        // Different parameter ordering (slot order matters).
        let reordered = PlanShapeFingerprintBuilder::new()
            .push_parameter(0x02, true, 0)
            .push_parameter(0x01, false, 0)
            .push_cardinality(0, CardinalityBucket::classify(100))
            .finish();
        assert_ne!(base, reordered);
    }

    #[test]
    fn plan_cache_key_singleton_rejects_shape_fingerprint() {
        let bind = binding(1, 1, 0xAA, 1, 0xBB);
        let err =
            PlanCacheKey::build(&bind, PlanClass::Singleton, shaped_fingerprint()).unwrap_err();
        assert_eq!(err, PlanCacheKeyError::SingletonRejectsShapeFingerprint);
    }

    #[test]
    fn plan_cache_key_shaped_class_requires_fingerprint() {
        let bind = binding(1, 1, 0xAA, 1, 0xBB);
        let err = PlanCacheKey::build(
            &bind,
            PlanClass::ParameterShape,
            PlanShapeFingerprint::empty(),
        )
        .unwrap_err();
        assert_eq!(err, PlanCacheKeyError::ShapedPlanClassRequiresFingerprint);
    }

    #[test]
    fn plan_cache_key_rejects_unbound_identity_inputs() {
        let cases = [
            (
                binding(0, 1, 0xAA, 1, 0xBB),
                PlanCacheKeyError::ProcedureIdZero,
            ),
            (
                binding(1, 0, 0xAA, 1, 0xBB),
                PlanCacheKeyError::CatalogVersionZero,
            ),
            (
                binding(1, 1, 0x00, 1, 0xBB),
                PlanCacheKeyError::ContractHashZero,
            ),
            (
                binding(1, 1, 0xAA, 0, 0xBB),
                PlanCacheKeyError::StatsVersionZero,
            ),
            (
                binding(1, 1, 0xAA, 1, 0x00),
                PlanCacheKeyError::PolicyVersionZero,
            ),
        ];

        for (bind, expected) in cases {
            let err =
                PlanCacheKey::build(&bind, PlanClass::Singleton, PlanShapeFingerprint::empty())
                    .unwrap_err();
            assert_eq!(err, expected);
        }
    }

    #[test]
    fn plan_cache_key_is_deterministic() {
        let bind = binding(7, 3, 0xAA, 2, 0xBB);
        let lhs = PlanCacheKey::build(&bind, PlanClass::Singleton, PlanShapeFingerprint::empty())
            .expect("singleton + empty fingerprint is valid");
        let rhs = PlanCacheKey::build(&bind, PlanClass::Singleton, PlanShapeFingerprint::empty())
            .expect("singleton + empty fingerprint is valid");
        assert_eq!(lhs, rhs);
        assert_eq!(lhs.digest(), rhs.digest());
    }

    #[test]
    fn plan_cache_key_separates_on_every_version_field() {
        let base_bind = binding(7, 3, 0xAA, 2, 0xBB);
        let base = PlanCacheKey::build(
            &base_bind,
            PlanClass::Singleton,
            PlanShapeFingerprint::empty(),
        )
        .unwrap();

        // Different ContractHash.
        let other = PlanCacheKey::build(
            &binding(7, 3, 0xCC, 2, 0xBB),
            PlanClass::Singleton,
            PlanShapeFingerprint::empty(),
        )
        .unwrap();
        assert_ne!(base, other);
        assert_ne!(base.digest(), other.digest());

        // Different CatalogVersion.
        let other = PlanCacheKey::build(
            &binding(7, 4, 0xAA, 2, 0xBB),
            PlanClass::Singleton,
            PlanShapeFingerprint::empty(),
        )
        .unwrap();
        assert_ne!(base, other);
        assert_ne!(base.digest(), other.digest());

        // Different StatsVersion.
        let other = PlanCacheKey::build(
            &binding(7, 3, 0xAA, 9, 0xBB),
            PlanClass::Singleton,
            PlanShapeFingerprint::empty(),
        )
        .unwrap();
        assert_ne!(base, other);
        assert_ne!(base.digest(), other.digest());

        // Different PolicyVersion.
        let other = PlanCacheKey::build(
            &binding(7, 3, 0xAA, 2, 0xEE),
            PlanClass::Singleton,
            PlanShapeFingerprint::empty(),
        )
        .unwrap();
        assert_ne!(base, other);
        assert_ne!(base.digest(), other.digest());

        // Different ProcedureId.
        let other = PlanCacheKey::build(
            &binding(8, 3, 0xAA, 2, 0xBB),
            PlanClass::Singleton,
            PlanShapeFingerprint::empty(),
        )
        .unwrap();
        assert_ne!(base, other);
        assert_ne!(base.digest(), other.digest());
    }

    #[test]
    fn plan_cache_key_separates_on_plan_class_and_shape() {
        let bind = binding(7, 3, 0xAA, 2, 0xBB);
        let fp = shaped_fingerprint();

        let shape_key = PlanCacheKey::build(&bind, PlanClass::ParameterShape, fp).unwrap();
        let card_key = PlanCacheKey::build(&bind, PlanClass::Cardinality, fp).unwrap();
        let adaptive_key = PlanCacheKey::build(&bind, PlanClass::StatsAdaptive, fp).unwrap();

        assert_ne!(shape_key, card_key);
        assert_ne!(shape_key, adaptive_key);
        assert_ne!(card_key, adaptive_key);
        assert_ne!(shape_key.digest(), card_key.digest());
        assert_ne!(shape_key.digest(), adaptive_key.digest());

        // A different fingerprint within the same class also separates.
        let other_fp = PlanShapeFingerprintBuilder::new()
            .push_parameter(0x01, false, 0)
            .push_cardinality(0, CardinalityBucket::Bulk)
            .finish();
        let other_shape_key =
            PlanCacheKey::build(&bind, PlanClass::ParameterShape, other_fp).unwrap();
        assert_ne!(shape_key, other_shape_key);
        assert_ne!(shape_key.digest(), other_shape_key.digest());
    }

    #[test]
    fn plan_class_singleton_rejects_shape_inputs_helper() {
        assert!(!PlanClass::Singleton.allows_shape_fingerprint());
        assert!(PlanClass::ParameterShape.allows_shape_fingerprint());
        assert!(PlanClass::Cardinality.allows_shape_fingerprint());
        assert!(PlanClass::StatsAdaptive.allows_shape_fingerprint());
    }
}
