use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

use crate::contracts::{PolicyVersion, ProcedureContractBinding, StatsVersion};
use crate::digest::Sha256;

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

    pub(crate) const fn from_bytes(bytes: [u8; Self::LEN]) -> Self {
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
