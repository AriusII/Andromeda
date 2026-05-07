//! Comprehensive plan cache invalidation tests.
//!
//! These tests verify that `PlanCacheKey` identity ensures cache invalidation
//! when any of its 7 components change:
//! 1. ProcedureId
//! 2. ContractHash
//! 3. CatalogVersion
//! 4. StatsVersion
//! 5. PolicyVersion
//! 6. PlanClass
//! 7. PlanShapeFingerprint
//!
//! The "no-silent-drop" guarantee is the core invariant: when a component
//! changes, the resulting key MUST NOT equal the old key, MUST have a different
//! digest, and the runtime cache MUST NOT reuse plans across the boundary.

use andromeda_catalog::{
    AdvisoryEvidenceStatus, BoundedPlanCache, CardinalityBucket, EvidenceConfidence, EvidenceScore,
    PLAN_SELECTION_MAX_SCENARIO_EVIDENCE, PlanCacheError, PlanCacheKey, PlanCacheKeyError,
    PlanCacheMissReason, PlanCandidate, PlanCandidateId, PlanCandidateRank, PlanClass,
    PlanDecisionOutcome, PlanSelectionError, PlanShapeFingerprint, PlanShapeFingerprintBuilder,
    PolicyVersion, ProcedureContractBinding, ScenarioEvidence, ScenarioId, ScenarioKind,
    ScenarioTarget, StatsVersion, ValidityWindow, classify_advisory_evidence_for_key,
    select_minimal_plan,
};
use andromeda_observe::{CriticalDecisionKind, TraceId};
use andromeda_time::EngineTimestamp;
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

#[path = "plan_invalidation/advisory_evidence.rs"]
mod advisory_evidence;
#[path = "plan_invalidation/bounded_cache.rs"]
mod bounded_cache;
#[path = "plan_invalidation/key_identity.rs"]
mod key_identity;
#[path = "plan_invalidation/key_validation.rs"]
mod key_validation;

/// Helper to construct a test binding with all fields customizable.
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

/// Helper to construct a non-empty shaped fingerprint for testing.
fn shaped_fingerprint() -> PlanShapeFingerprint {
    PlanShapeFingerprintBuilder::new()
        .push_parameter(0x01, false, 0)
        .push_parameter(0x02, true, 0)
        .push_cardinality(0, CardinalityBucket::classify(100))
        .finish()
}

/// Helper to construct a different shaped fingerprint (different parameters).
fn shaped_fingerprint_alt() -> PlanShapeFingerprint {
    PlanShapeFingerprintBuilder::new()
        .push_parameter(0x03, false, 2)
        .push_cardinality(0, CardinalityBucket::classify(10_000))
        .finish()
}

fn ts(ms: u64) -> EngineTimestamp {
    EngineTimestamp::from_unix_millis(ms)
}

fn candidate(id: u64, plan_class: PlanClass, rank: u16, digest_byte: u8) -> PlanCandidate {
    PlanCandidate::new(
        PlanCandidateId::new(id).expect("candidate id must be non-zero"),
        plan_class,
        PlanCandidateRank::from_permille(rank).expect("rank must be bounded"),
        [digest_byte; 32],
    )
    .expect("candidate digest must be non-zero")
}

fn scenario_evidence_for_key(
    key: PlanCacheKey,
    scenario_id: u64,
    score: u16,
    confidence: u16,
) -> ScenarioEvidence {
    ScenarioEvidence::new(
        ScenarioId::new(scenario_id).expect("scenario id must be non-zero"),
        ScenarioKind::Microbenchmark,
        ScenarioTarget {
            procedure_id: key.procedure_id,
            catalog_version: key.catalog_version,
            stats_version: key.stats_version,
            plan_class: Some(key.plan_class),
            contract_hash: Some(key.contract_hash),
        },
        EvidenceScore::from_permille(score).expect("score must be bounded"),
        EvidenceConfidence::from_permille(confidence).expect("confidence must be bounded"),
        ValidityWindow::new(ts(100), ts(200)).expect("validity window must be bounded"),
    )
    .expect("scenario evidence target must be valid")
}
