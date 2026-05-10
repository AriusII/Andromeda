#![allow(dead_code)]

use andromeda_observability::TraceId;
use andromeda_plan_cache::{
    AdvisoryEvidenceSummary, CardinalityBucket, PlanCacheKey, PlanCandidate, PlanCandidateId,
    PlanCandidateRank, PlanClass, PlanSelectionError, PlanSelectionOutcome, PlanShapeFingerprint,
    PlanShapeFingerprintBuilder, select_minimal_plan_with_advisory_evidence,
};
use andromeda_procedure_contract::{PolicyVersion, ProcedureContractBinding, StatsVersion};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

pub fn binding(
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

pub fn shaped_fingerprint() -> PlanShapeFingerprint {
    PlanShapeFingerprintBuilder::new()
        .push_parameter(0x01, false, 0)
        .push_parameter(0x02, true, 0)
        .push_cardinality(0, CardinalityBucket::classify(100))
        .finish()
}

pub fn shaped_fingerprint_alt() -> PlanShapeFingerprint {
    PlanShapeFingerprintBuilder::new()
        .push_parameter(0x03, false, 2)
        .push_cardinality(0, CardinalityBucket::classify(10_000))
        .finish()
}

pub fn candidate(id: u64, plan_class: PlanClass, rank: u16, digest_byte: u8) -> PlanCandidate {
    PlanCandidate::new(
        PlanCandidateId::new(id).expect("candidate id must be non-zero"),
        plan_class,
        PlanCandidateRank::from_permille(rank).expect("rank must be bounded"),
        [digest_byte; 32],
    )
    .expect("candidate digest must be non-zero")
}

pub fn select_minimal_plan(
    key: PlanCacheKey,
    candidates: &[PlanCandidate],
    trace_id: TraceId,
) -> Result<PlanSelectionOutcome, PlanSelectionError> {
    select_minimal_plan_with_advisory_evidence(
        key,
        candidates,
        AdvisoryEvidenceSummary::empty(),
        trace_id,
    )
}

#[allow(dead_code)]
pub fn _type_usage_sentinel(
    _binding: ProcedureContractBinding,
    _policy: PolicyVersion,
    _stats: StatsVersion,
    _catalog: CatalogVersion,
    _contract: ContractHash,
    _procedure: ProcedureId,
) {
}
