//! V0 plan-class and plan-cache key scaffold.
//!
//! **Status: SCAFFOLD ONLY.**  This module does not implement an optimizer,
//! does not perform plan compilation, does not maintain a runtime cache, and
//! does not make any cost-based decisions.  It only defines the bounded set of
//! identity inputs that a future Procedure-only optimizer must hash into a
//! plan-cache key, so that plan reuse is deterministic and traceable.
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
//! 5. **E5 runtime-cache decision.**  V0.5 keeps this module scaffold-only.
//!    A runtime cache is explicitly deferred until catalog lifecycle,
//!    statistics publication, policy-version publication, and optimizer
//!    DecisionTrace semantics are ready to prove correct invalidation.  Future
//!    cache implementations must be bounded, must key on every
//!    [`PlanCacheKey`] field, and must emit a trace containing the key digest
//!    plus the plan-class/evidence inputs that justified reuse or miss.
//!
//! ## What this module deliberately does NOT do
//!
//! - It does not select a plan.
//! - It does not score plans.
//! - It does not compile SRPL.
//! - It does not consult statistics or histograms.
//! - It does not own a cache: it only defines and validates the *key inputs*.
//! - It does not interpret SQL; the project remains Procedure-only.
//!
//! Any later optimizer crate can depend on these types without re-deriving
//! the key shape, ensuring decision traces remain reproducible across
//! catalog / stats / policy upgrades.

use andromeda_core::{CatalogVersion, ContractHash, ProcedureId};

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
