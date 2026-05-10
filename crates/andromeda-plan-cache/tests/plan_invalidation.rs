//! Plan-cache bridge tests for advisory evidence behavior.
//!
//! Pure key, cache, deterministic selection, advisory classification, and trace
//! behavior belong with the plan-cache owner crate.

use andromeda_observability::{CriticalDecisionKind, TraceId};
use andromeda_plan_cache::{
    AdvisoryEvidenceIdentity, AdvisoryEvidenceStatus, AdvisoryEvidenceSummary,
    AdvisoryEvidenceSummaryBuilder, BoundedPlanCache, CardinalityBucket,
    PLAN_SELECTION_MAX_SCENARIO_EVIDENCE, PlanCacheKey, PlanCacheMissReason, PlanCandidate,
    PlanCandidateId, PlanCandidateRank, PlanClass, PlanDecisionOutcome, PlanSelectionError,
    PlanShapeFingerprint, PlanShapeFingerprintBuilder, classify_advisory_identity_for_key,
    select_minimal_plan_with_advisory_evidence,
};
use andromeda_procedure_contract::{PolicyVersion, ProcedureContractBinding, StatsVersion};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

#[path = "plan_invalidation/advisory_evidence.rs"]
mod advisory_evidence;

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

fn candidate(id: u64, plan_class: PlanClass, rank: u16, digest_byte: u8) -> PlanCandidate {
    PlanCandidate::new(
        PlanCandidateId::new(id).expect("candidate id must be non-zero"),
        plan_class,
        PlanCandidateRank::from_permille(rank).expect("rank must be bounded"),
        [digest_byte; 32],
    )
    .expect("candidate digest must be non-zero")
}

fn evidence_digest(byte: u8) -> [u8; 32] {
    [byte; 32]
}

fn advisory_identity_for_key(key: PlanCacheKey) -> AdvisoryEvidenceIdentity {
    AdvisoryEvidenceIdentity {
        procedure_id: key.procedure_id,
        catalog_version: key.catalog_version,
        stats_version: key.stats_version,
        plan_class: Some(key.plan_class),
        contract_hash: Some(key.contract_hash),
    }
}

fn advisory_summary(
    supplied_count: usize,
    accepted: &[(u16, u16, [u8; 32])],
    rejected: &[AdvisoryEvidenceStatus],
) -> AdvisoryEvidenceSummary {
    let mut builder =
        AdvisoryEvidenceSummaryBuilder::new(supplied_count).expect("summary is bounded");
    for (score, confidence, digest) in accepted {
        builder.record_accepted_advisory(*score, *confidence, *digest);
    }
    for status in rejected {
        builder.record_rejected(*status);
    }
    builder.finish()
}

#[test]
fn stats_version_bump_invalidates_cache_entry_and_decision_trace_explains_miss() {
    let base_binding = binding(1_200, 90, 0xA5, 45, 0xB5);
    let base_fingerprint = shaped_fingerprint();
    let base_key = PlanCacheKey::build(&base_binding, PlanClass::Cardinality, base_fingerprint)
        .expect("base cardinality key must be valid");
    let same_key = PlanCacheKey::build(&base_binding, PlanClass::Cardinality, base_fingerprint)
        .expect("same binding must rebuild the same key");
    assert_eq!(base_key, same_key);
    assert_eq!(base_key.digest(), same_key.digest());

    let selected = select_minimal_plan_with_advisory_evidence(
        base_key,
        &[candidate(71, PlanClass::Cardinality, 100, 0x71)],
        AdvisoryEvidenceSummary::empty(),
        TraceId::new(17_001),
    )
    .expect("matching cardinality candidate should select");

    let mut cache = BoundedPlanCache::new(2).expect("bounded cache capacity should be valid");
    cache
        .insert_selection(&selected, TraceId::new(17_002))
        .expect("selected plan can be cached");

    let hit = cache
        .lookup(base_key, TraceId::new(17_003))
        .expect("same full key lookup should produce hit trace");
    assert!(hit.is_hit());
    assert_eq!(hit.trace().cache_miss_reason(), None);

    let bumped_stats_key = PlanCacheKey::build(
        &binding(1_200, 90, 0xA5, 46, 0xB5),
        PlanClass::Cardinality,
        base_fingerprint,
    )
    .expect("bumped StatsVersion key must be valid");
    assert_ne!(base_key, bumped_stats_key);
    assert_ne!(base_key.digest(), bumped_stats_key.digest());

    let miss = cache
        .lookup(bumped_stats_key, TraceId::new(17_004))
        .expect("StatsVersion miss should produce trace evidence");
    assert!(!miss.is_hit());
    assert_eq!(
        miss.trace().cache_miss_reason(),
        Some(PlanCacheMissReason::StatsVersionMismatch)
    );

    let decision_trace = miss.trace().as_decision_trace();
    assert_eq!(decision_trace.decision, CriticalDecisionKind::PlanSelection);
    assert!(decision_trace.has_explanation());
    assert!(
        decision_trace
            .reason
            .contains("version_binding=ContractHash+CatalogVersion+StatsVersion")
    );
    assert!(decision_trace.reason.contains("outcome=cache-miss"));
    assert!(decision_trace.reason.contains("catalog_version=90"));
    assert!(decision_trace.reason.contains("stats_version=46"));
    assert!(
        decision_trace
            .reason
            .contains("cache_miss_reason=stats-version-mismatch")
    );
}

#[test]
fn plan_selection_decision_trace_records_advisory_evidence_counts_and_best_digest() {
    let binding = binding(1_201, 91, 0xA6, 47, 0xB6);
    let key = PlanCacheKey::build(&binding, PlanClass::StatsAdaptive, shaped_fingerprint())
        .expect("stats-adaptive key must be valid");
    let candidates = [
        candidate(81, PlanClass::StatsAdaptive, 100, 0x81),
        candidate(82, PlanClass::StatsAdaptive, 800, 0x82),
    ];
    let best_digest = evidence_digest(0x82);
    let stats_mismatch = AdvisoryEvidenceIdentity {
        stats_version: StatsVersion::new(key.stats_version.get() + 1),
        ..advisory_identity_for_key(key)
    };
    assert_eq!(
        classify_advisory_identity_for_key(&key, stats_mismatch, false, false),
        AdvisoryEvidenceStatus::StatsVersionMismatch
    );
    let advisory = advisory_summary(
        3,
        &[(600, 700, evidence_digest(0x81)), (900, 850, best_digest)],
        &[AdvisoryEvidenceStatus::StatsVersionMismatch],
    );

    let selection = select_minimal_plan_with_advisory_evidence(
        key,
        &candidates,
        advisory,
        TraceId::new(17_005),
    )
    .expect("advisory evidence should not block deterministic selection");

    assert_eq!(
        selection.selected().plan_id(),
        PlanCandidateId::new(81).expect("candidate id must be non-zero")
    );
    let advisory = selection.trace().advisory_evidence();
    assert_eq!(advisory.supplied_count(), 3);
    assert_eq!(advisory.accepted_count(), 2);
    assert_eq!(advisory.rejected_count(), 1);
    assert_eq!(
        advisory.status_count(AdvisoryEvidenceStatus::AcceptedAdvisory),
        2
    );
    assert_eq!(
        advisory.status_count(AdvisoryEvidenceStatus::StatsVersionMismatch),
        1
    );
    assert_eq!(advisory.best_score_permille(), Some(900));
    assert_eq!(advisory.best_confidence_permille(), Some(850));
    assert_eq!(advisory.best_evidence_digest(), Some(best_digest));

    let decision_trace = selection.trace().as_decision_trace();
    assert_eq!(decision_trace.decision, CriticalDecisionKind::PlanSelection);
    assert!(decision_trace.has_explanation());
    assert!(decision_trace.reason.contains("outcome=selected"));
    assert!(decision_trace.reason.contains("selected_plan_id=81"));
    assert!(
        decision_trace
            .reason
            .contains("scenario_evidence_supplied=3")
    );
    assert!(
        decision_trace
            .reason
            .contains("scenario_evidence_accepted_advisory=2")
    );
    assert!(
        decision_trace
            .reason
            .contains("scenario_evidence_rejected=1")
    );
    assert!(decision_trace.reason.contains("accepted-advisory:2"));
    assert!(decision_trace.reason.contains("stats-version-mismatch:1"));
    assert!(decision_trace.reason.contains("best_evidence_digest="));
    assert!(decision_trace.reason.contains("advisory_only=true"));
}
