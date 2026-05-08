mod support;

use andromeda_observe::TraceId;
use andromeda_plan_cache::{
    AdvisoryEvidenceStatus, AdvisoryEvidenceSummary, AdvisoryEvidenceSummaryBuilder,
    PlanCandidateId, PlanClass, PlanSelectionError, select_minimal_plan_with_advisory_evidence,
};

use support::{binding, candidate, shaped_fingerprint};

#[test]
fn selection_prefers_lowest_rank_within_requested_plan_class() {
    let key = andromeda_plan_cache::PlanCacheKey::build(
        &binding(1_100, 90, 0xAA, 21, 0xBB),
        PlanClass::StatsAdaptive,
        shaped_fingerprint(),
    )
    .unwrap();
    let candidates = [
        candidate(11, PlanClass::StatsAdaptive, 900, 0x11),
        candidate(12, PlanClass::Cardinality, 0, 0x12),
        candidate(13, PlanClass::StatsAdaptive, 100, 0x13),
    ];

    let outcome = select_minimal_plan_with_advisory_evidence(
        key,
        &candidates,
        AdvisoryEvidenceSummary::empty(),
        TraceId::new(18_001),
    )
    .unwrap();

    assert_eq!(
        outcome.selected().plan_id(),
        PlanCandidateId::new(13).unwrap()
    );
    assert_eq!(outcome.trace().matching_candidate_count(), 2);
    assert_eq!(outcome.trace().candidate_count(), 3);
}

#[test]
fn selection_breaks_equal_rank_ties_by_candidate_id() {
    let key = andromeda_plan_cache::PlanCacheKey::build(
        &binding(1_101, 90, 0xAA, 21, 0xBB),
        PlanClass::ParameterShape,
        shaped_fingerprint(),
    )
    .unwrap();
    let candidates = [
        candidate(22, PlanClass::ParameterShape, 100, 0x22),
        candidate(21, PlanClass::ParameterShape, 100, 0x21),
    ];

    let outcome = select_minimal_plan_with_advisory_evidence(
        key,
        &candidates,
        AdvisoryEvidenceSummary::empty(),
        TraceId::new(18_002),
    )
    .unwrap();

    assert_eq!(
        outcome.selected().plan_id(),
        PlanCandidateId::new(21).unwrap()
    );
}

#[test]
fn selection_rejects_when_no_candidate_matches_plan_class() {
    let key = andromeda_plan_cache::PlanCacheKey::build(
        &binding(1_102, 90, 0xAA, 21, 0xBB),
        PlanClass::Cardinality,
        shaped_fingerprint(),
    )
    .unwrap();
    let candidates = [candidate(31, PlanClass::ParameterShape, 100, 0x31)];

    let error = select_minimal_plan_with_advisory_evidence(
        key,
        &candidates,
        AdvisoryEvidenceSummary::empty(),
        TraceId::new(18_003),
    )
    .unwrap_err();

    assert_eq!(error, PlanSelectionError::NoCandidateForPlanClass);
}

#[test]
fn selection_trace_preserves_advisory_summary_without_granting_authority() {
    let key = andromeda_plan_cache::PlanCacheKey::build(
        &binding(1_103, 90, 0xAA, 21, 0xBB),
        PlanClass::StatsAdaptive,
        shaped_fingerprint(),
    )
    .unwrap();
    let candidates = [candidate(41, PlanClass::StatsAdaptive, 100, 0x41)];
    let mut summary = AdvisoryEvidenceSummaryBuilder::new(2).unwrap();
    summary.record_accepted_advisory(800, 700, [0x81; 32]);
    summary.record_rejected(AdvisoryEvidenceStatus::StatsVersionMismatch);

    let outcome = select_minimal_plan_with_advisory_evidence(
        key,
        &candidates,
        summary.finish(),
        TraceId::new(18_004),
    )
    .unwrap();

    let advisory = outcome.trace().advisory_evidence();
    assert_eq!(advisory.supplied_count(), 2);
    assert_eq!(advisory.accepted_count(), 1);
    assert_eq!(advisory.rejected_count(), 1);
    assert_eq!(
        advisory.status_count(AdvisoryEvidenceStatus::AcceptedAdvisory),
        1
    );
    assert_eq!(
        advisory.status_count(AdvisoryEvidenceStatus::StatsVersionMismatch),
        1
    );
    assert_eq!(advisory.best_score_permille(), Some(800));
    assert_eq!(advisory.best_confidence_permille(), Some(700));
    assert_eq!(advisory.best_evidence_digest(), Some([0x81; 32]));
    assert_eq!(
        outcome.selected().plan_id(),
        PlanCandidateId::new(41).unwrap()
    );
}
