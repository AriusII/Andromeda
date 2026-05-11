use std::borrow::Borrow;

use andromeda_decision_trace::VersionBinding;
use andromeda_digest::Sha256;
use andromeda_procedure_contract::{PolicyVersion, ProcedureContractBinding, StatsVersion};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

use crate::PlanCacheKeyError;

pub const PLAN_CACHE_KEY_SCHEMA_VERSION: u16 = 0;

const PLAN_SHAPE_DOMAIN: &[u8] = b"andromeda.plan_cache.shape.v0";
const PLAN_CACHE_DOMAIN: &[u8] = b"andromeda.plan_cache.key.v0";

/// Coarse plan classification carried in every [`PlanCacheKey`].
///
/// # Wire-tag stability contract
///
/// Tags 0x01–0x04 are **legacy / internal** variants introduced before the P09
/// specification.  Their wire values are frozen and **must never be reused** for
/// new variants, even if a legacy variant is deprecated.
///
/// Tags 0x05–0x0C are the **P09 spec classes** (GAP-3 reconciliation, audit
/// A4).  They are assigned in spec order and are likewise immutable once
/// published.
///
/// New variants, when added in future phases, must claim tags starting from
/// 0x0D to avoid silent wire-level collisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum PlanClass {
    // ── Legacy / internal classification ─────────────────────────────────────
    // Wire tags MUST NOT be reused even if a variant is later deprecated.
    /// Single-result plan with no shape variation.  Uses the empty
    /// [`PlanShapeFingerprint`].
    Singleton = 0x01,
    /// Shape driven by the parameter-type signature of the procedure.
    ParameterShape = 0x02,
    /// Shape driven by the input cardinality bucket.
    Cardinality = 0x03,
    /// Shape selected with statistics-adaptive hints (non-authoritative).
    StatsAdaptive = 0x04,

    // ── P09 spec plan classes (GAP-3 reconciliation) ──────────────────────────
    // Defined in P09_STATISTICS_OPTIMIZER_PROCEDURE_STORE_V0.md §Tâches détaillées.
    // Classification helpers that map row-count buckets or object-shape signals
    // to these variants live in the optimizer layer; the cache itself is class-agnostic.
    /// Generic plan with no row-count or shape specialisation.
    Generic = 0x05,
    /// Specialised for small row-count inputs (bucket: rows ≤ 64).
    Small = 0x06,
    /// Specialised for medium row-count inputs (bucket: 65 ≤ rows ≤ 4 096).
    Medium = 0x07,
    /// Specialised for large row-count inputs (bucket: 4 097 ≤ rows ≤ 262 144).
    Large = 0x08,
    /// Specialised for skewed cardinality distributions (NDV / skew signal).
    Skewed = 0x09,
    /// Specialised for small structured-object payloads.
    StructuredObjectSmall = 0x0A,
    /// Specialised for large structured-object payloads.
    StructuredObjectLarge = 0x0B,
    /// Plan class reserved for maintenance and administrative procedures.
    Maintenance = 0x0C,
}

impl PlanClass {
    /// Total number of variants (4 legacy + 8 P09 spec).
    pub const VARIANT_COUNT: usize = 12;

    /// Stable wire tag for this variant.
    ///
    /// The mapping is intentionally `const` and exhaustive so that any future
    /// variant addition forces a compiler error here, preventing silent tag
    /// drift.
    pub const fn as_tag(self) -> u8 {
        match self {
            // Legacy variants — tags frozen at 0x01–0x04
            Self::Singleton => 0x01,
            Self::ParameterShape => 0x02,
            Self::Cardinality => 0x03,
            Self::StatsAdaptive => 0x04,
            // P09 spec variants — tags frozen at 0x05–0x0C
            Self::Generic => 0x05,
            Self::Small => 0x06,
            Self::Medium => 0x07,
            Self::Large => 0x08,
            Self::Skewed => 0x09,
            Self::StructuredObjectSmall => 0x0A,
            Self::StructuredObjectLarge => 0x0B,
            Self::Maintenance => 0x0C,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CardinalityBucket {
    Zero,
    One,
    Small,
    Medium,
    Large,
    Bulk,
}

impl CardinalityBucket {
    pub const VARIANT_COUNT: usize = 6;

    pub const fn classify(row_count: u64) -> Self {
        if row_count == 0 {
            Self::Zero
        } else if row_count == 1 {
            Self::One
        } else if row_count <= 64 {
            Self::Small
        } else if row_count <= 4_096 {
            Self::Medium
        } else if row_count <= 262_144 {
            Self::Large
        } else {
            Self::Bulk
        }
    }

    pub const fn as_tag(self) -> u8 {
        match self {
            Self::Zero => 0x10,
            Self::One => 0x11,
            Self::Small => 0x12,
            Self::Medium => 0x13,
            Self::Large => 0x14,
            Self::Bulk => 0x15,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlanShapeFingerprint([u8; Self::LEN]);

impl PlanShapeFingerprint {
    pub const LEN: usize = 32;

    pub const fn empty() -> Self {
        Self([0; Self::LEN])
    }

    const fn from_bytes(bytes: [u8; Self::LEN]) -> Self {
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

    pub fn push_parameter(mut self, type_tag: u8, nullable: bool, structured_arity: u16) -> Self {
        self.hasher.update(&[0xA0]);
        self.hasher.update(&[type_tag]);
        self.hasher.update(&[u8::from(nullable)]);
        self.hasher.update(&structured_arity.to_le_bytes());
        self.parameter_count = self.parameter_count.saturating_add(1);
        self
    }

    pub fn push_cardinality(mut self, slot_index: u16, bucket: CardinalityBucket) -> Self {
        self.hasher.update(&[0xA1]);
        self.hasher.update(&slot_index.to_le_bytes());
        self.hasher.update(&[bucket.as_tag()]);
        self.cardinality_count = self.cardinality_count.saturating_add(1);
        self
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanCacheKey {
    pub procedure_id: ProcedureId,
    pub contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
    pub stats_version: StatsVersion,
    pub policy_version: PolicyVersion,
    pub plan_class: PlanClass,
    pub shape_fingerprint: PlanShapeFingerprint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlanCachePublicationIdentity {
    pub procedure_id: ProcedureId,
    pub contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
}

impl PlanCachePublicationIdentity {
    pub const fn new(
        procedure_id: ProcedureId,
        contract_hash: ContractHash,
        catalog_version: CatalogVersion,
    ) -> Self {
        Self {
            procedure_id,
            contract_hash,
            catalog_version,
        }
    }

    pub const fn procedure_id(self) -> ProcedureId {
        self.procedure_id
    }

    pub const fn contract_hash(self) -> ContractHash {
        self.contract_hash
    }

    pub const fn catalog_version(self) -> CatalogVersion {
        self.catalog_version
    }

    pub fn covers_key(self, key: PlanCacheKey) -> bool {
        self.procedure_id == key.procedure_id
            && self.contract_hash == key.contract_hash
            && self.catalog_version == key.catalog_version
    }

    pub fn invalidates_key(self, key: PlanCacheKey) -> bool {
        self.procedure_id == key.procedure_id && !self.covers_key(key)
    }
}

impl PlanCacheKey {
    pub fn build(
        binding: impl Borrow<ProcedureContractBinding>,
        plan_class: PlanClass,
        shape_fingerprint: PlanShapeFingerprint,
    ) -> Result<Self, PlanCacheKeyError> {
        let binding = *binding.borrow();
        if binding.procedure_id.get() == 0 {
            return Err(PlanCacheKeyError::ProcedureIdZero);
        }
        if binding.catalog_version.get() == 0 {
            return Err(PlanCacheKeyError::CatalogVersionZero);
        }
        if binding.contract_hash.is_zero() {
            return Err(PlanCacheKeyError::ContractHashZero);
        }
        if binding.stats_version.is_zero() {
            return Err(PlanCacheKeyError::StatsVersionZero);
        }
        if binding.policy_version.is_zero() {
            return Err(PlanCacheKeyError::PolicyVersionZero);
        }
        match (plan_class, shape_fingerprint.is_empty()) {
            (PlanClass::Singleton, false) => {
                return Err(PlanCacheKeyError::SingletonRejectsShapeFingerprint);
            },
            (other, true) if other != PlanClass::Singleton => {
                return Err(PlanCacheKeyError::ShapedPlanClassRequiresFingerprint);
            },
            _ => {},
        }

        Ok(Self {
            procedure_id: binding.procedure_id,
            contract_hash: binding.contract_hash,
            catalog_version: binding.catalog_version,
            stats_version: binding.stats_version,
            policy_version: binding.policy_version,
            plan_class,
            shape_fingerprint,
        })
    }

    pub const fn binding(self) -> ProcedureContractBinding {
        ProcedureContractBinding {
            procedure_id: self.procedure_id,
            catalog_version: self.catalog_version,
            contract_hash: self.contract_hash,
            stats_version: self.stats_version,
            policy_version: self.policy_version,
        }
    }

    pub const fn procedure_id(self) -> ProcedureId {
        self.procedure_id
    }

    pub const fn contract_hash(self) -> ContractHash {
        self.contract_hash
    }

    pub const fn catalog_version(self) -> CatalogVersion {
        self.catalog_version
    }

    pub const fn stats_version(self) -> StatsVersion {
        self.stats_version
    }

    pub const fn policy_version(self) -> PolicyVersion {
        self.policy_version
    }

    pub const fn plan_class(self) -> PlanClass {
        self.plan_class
    }

    pub const fn shape_fingerprint(self) -> PlanShapeFingerprint {
        self.shape_fingerprint
    }

    pub const fn publication_identity(self) -> PlanCachePublicationIdentity {
        PlanCachePublicationIdentity::new(
            self.procedure_id,
            self.contract_hash,
            self.catalog_version,
        )
    }

    pub const fn version_binding(self) -> VersionBinding {
        VersionBinding::for_procedure(self.binding())
    }

    pub fn digest(self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(PLAN_CACHE_DOMAIN);
        hasher.update(&[0xC0]);
        hasher.update(&self.procedure_id.get().to_le_bytes());
        hasher.update(&[0xC1]);
        let contract_hash = self.contract_hash.as_bytes();
        hasher.update(&contract_hash);
        hasher.update(&[0xC2]);
        hasher.update(&self.catalog_version.get().to_le_bytes());
        hasher.update(&[0xC3]);
        hasher.update(&self.stats_version.get().to_le_bytes());
        hasher.update(&[0xC4]);
        let policy_version = self.policy_version.as_bytes();
        hasher.update(&policy_version);
        hasher.update(&[0xC5]);
        hasher.update(&[self.plan_class.as_tag()]);
        hasher.update(&[0xC6]);
        let shape = self.shape_fingerprint.as_bytes();
        hasher.update(&shape);
        hasher.finalize()
    }
}

impl From<PlanCacheKey> for PlanCachePublicationIdentity {
    fn from(value: PlanCacheKey) -> Self {
        value.publication_identity()
    }
}
