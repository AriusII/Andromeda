use super::*;

#[test]
fn minimal_plan_selection_keeps_scenario_evidence_advisory_only() {
    let bind = binding(800, 60, 0xAA, 12, 0xBB);
    let key = PlanCacheKey::build(bind, PlanClass::StatsAdaptive, shaped_fingerprint())
        .expect("valid stats-adaptive key");
    let candidates = [
        candidate(20, PlanClass::StatsAdaptive, 400, 0x20),
        candidate(10, PlanClass::StatsAdaptive, 100, 0x10),
        candidate(30, PlanClass::Cardinality, 0, 0x30),
    ];
    let identity = advisory_identity_for_key(key);

    assert_eq!(
        classify_advisory_identity_for_key(&key, identity, false, false),
        AdvisoryEvidenceStatus::AcceptedAdvisory
    );

    let selection = select_minimal_plan_with_advisory_evidence(
        key,
        &candidates,
        advisory_summary(1, &[(1_000, 1_000, evidence_digest(0x01))], &[]),
        TraceId::new(16_001),
    )
    .expect("bounded candidates and evidence should select");

    assert_eq!(
        selection.selected().plan_id(),
        PlanCandidateId::new(10).unwrap(),
        "static rank remains the selector authority"
    );
    assert_eq!(selection.trace().advisory_evidence().supplied_count(), 1);
    assert_eq!(selection.trace().advisory_evidence().accepted_count(), 1);
    assert_eq!(selection.trace().outcome(), PlanDecisionOutcome::Selected);

    let trace = selection.trace().as_decision_trace();
    assert_eq!(trace.decision, CriticalDecisionKind::PlanSelection);
    assert!(trace.has_explanation());
    assert!(trace.reason.contains("advisory_only=true"));
    assert!(trace.reason.contains("contract_hash="));
    assert!(
        trace
            .reason
            .contains("version_binding=ContractHash+CatalogVersion+StatsVersion")
    );
    assert!(trace.reason.contains("stats_version=12"));
    assert!(trace.reason.contains("policy_version="));
    assert!(
        trace
            .reason
            .contains("scenario_evidence_statuses=accepted-advisory:1")
    );
}

#[test]
fn minimal_plan_selection_traces_rejected_stale_scenario_evidence_without_changing_selection() {
    let bind = binding(801, 61, 0xAA, 13, 0xBB);
    let key = PlanCacheKey::build(bind, PlanClass::ParameterShape, shaped_fingerprint())
        .expect("valid parameter-shape key");
    let candidates = [
        candidate(11, PlanClass::ParameterShape, 200, 0x11),
        candidate(12, PlanClass::ParameterShape, 300, 0x12),
    ];
    let stale_target_evidence = AdvisoryEvidenceIdentity {
        stats_version: StatsVersion::new(key.stats_version.get() + 1),
        ..advisory_identity_for_key(key)
    };

    assert_eq!(
        classify_advisory_identity_for_key(&key, stale_target_evidence, false, false),
        AdvisoryEvidenceStatus::StatsVersionMismatch
    );

    let selection = select_minimal_plan_with_advisory_evidence(
        key,
        &candidates,
        advisory_summary(1, &[], &[AdvisoryEvidenceStatus::StatsVersionMismatch]),
        TraceId::new(16_002),
    )
    .expect("stale evidence should be rejected, not fatal");

    assert_eq!(
        selection.selected().plan_id(),
        PlanCandidateId::new(11).unwrap()
    );
    assert_eq!(selection.trace().advisory_evidence().accepted_count(), 0);
    assert_eq!(selection.trace().advisory_evidence().rejected_count(), 1);
    assert_eq!(
        selection
            .trace()
            .advisory_evidence()
            .status_count(AdvisoryEvidenceStatus::StatsVersionMismatch),
        1
    );
    assert!(
        selection
            .trace()
            .as_decision_trace()
            .reason
            .contains("scenario_evidence_rejected=1")
    );
}

#[test]
fn minimal_plan_selection_rejects_stale_catalog_scenario_evidence_without_changing_selection() {
    let bind = binding(802, 62, 0xAA, 14, 0xBB);
    let key = PlanCacheKey::build(bind, PlanClass::ParameterShape, shaped_fingerprint())
        .expect("valid parameter-shape key");
    let candidates = [
        candidate(21, PlanClass::ParameterShape, 200, 0x21),
        candidate(22, PlanClass::ParameterShape, 300, 0x22),
    ];
    let stale_catalog_evidence = AdvisoryEvidenceIdentity {
        catalog_version: CatalogVersion::new(key.catalog_version.get() + 1),
        ..advisory_identity_for_key(key)
    };

    assert_eq!(
        classify_advisory_identity_for_key(&key, stale_catalog_evidence, false, false),
        AdvisoryEvidenceStatus::CatalogVersionMismatch
    );

    let selection = select_minimal_plan_with_advisory_evidence(
        key,
        &candidates,
        advisory_summary(1, &[], &[AdvisoryEvidenceStatus::CatalogVersionMismatch]),
        TraceId::new(16_013),
    )
    .expect("stale catalog evidence should be rejected, not fatal");

    assert_eq!(
        selection.selected().plan_id(),
        PlanCandidateId::new(21).unwrap()
    );
    assert_eq!(selection.trace().advisory_evidence().accepted_count(), 0);
    assert_eq!(selection.trace().advisory_evidence().rejected_count(), 1);
    assert_eq!(
        selection
            .trace()
            .advisory_evidence()
            .status_count(AdvisoryEvidenceStatus::CatalogVersionMismatch),
        1
    );
    assert!(
        selection
            .trace()
            .as_decision_trace()
            .reason
            .contains("catalog_version=62")
    );
}

#[test]
fn scenario_evidence_key_mismatches_are_rejected_and_traced_without_selecting() {
    let bind = binding(950, 82, 0xAA, 18, 0xBB);
    let key = PlanCacheKey::build(bind, PlanClass::StatsAdaptive, shaped_fingerprint())
        .expect("valid stats-adaptive key");
    let candidates = [
        candidate(30, PlanClass::StatsAdaptive, 100, 0x30),
        candidate(31, PlanClass::StatsAdaptive, 900, 0x31),
    ];

    let plan_class_mismatch = AdvisoryEvidenceIdentity {
        plan_class: Some(PlanClass::Cardinality),
        ..advisory_identity_for_key(key)
    };
    let missing_contract = AdvisoryEvidenceIdentity {
        contract_hash: None,
        ..advisory_identity_for_key(key)
    };
    let contract_mismatch = AdvisoryEvidenceIdentity {
        contract_hash: Some(ContractHash::new([0xCC; ContractHash::LEN])),
        ..advisory_identity_for_key(key)
    };
    let expired = advisory_identity_for_key(key);
    let not_yet_valid = advisory_identity_for_key(key);

    assert_eq!(
        classify_advisory_identity_for_key(&key, plan_class_mismatch, false, false),
        AdvisoryEvidenceStatus::PlanClassMismatch
    );
    assert_eq!(
        classify_advisory_identity_for_key(&key, missing_contract, false, false),
        AdvisoryEvidenceStatus::ContractHashMissing
    );
    assert_eq!(
        classify_advisory_identity_for_key(&key, contract_mismatch, false, false),
        AdvisoryEvidenceStatus::ContractHashMismatch
    );
    assert_eq!(
        classify_advisory_identity_for_key(&key, expired, false, false),
        AdvisoryEvidenceStatus::AcceptedAdvisory
    );
    assert_eq!(
        classify_advisory_identity_for_key(&key, not_yet_valid, false, false),
        AdvisoryEvidenceStatus::AcceptedAdvisory
    );

    let selection = select_minimal_plan_with_advisory_evidence(
        key,
        &candidates,
        advisory_summary(
            5,
            &[],
            &[
                AdvisoryEvidenceStatus::PlanClassMismatch,
                AdvisoryEvidenceStatus::ContractHashMissing,
                AdvisoryEvidenceStatus::ContractHashMismatch,
                AdvisoryEvidenceStatus::Expired,
                AdvisoryEvidenceStatus::NotYetValid,
            ],
        ),
        TraceId::new(16_013),
    )
    .expect("rejected advisory evidence must not block deterministic selection");

    assert_eq!(
        selection.selected().plan_id(),
        PlanCandidateId::new(30).unwrap()
    );
    assert_eq!(selection.trace().advisory_evidence().accepted_count(), 0);
    assert_eq!(selection.trace().advisory_evidence().rejected_count(), 5);
    assert_eq!(
        selection
            .trace()
            .advisory_evidence()
            .status_count(AdvisoryEvidenceStatus::PlanClassMismatch),
        1
    );
    assert_eq!(
        selection
            .trace()
            .advisory_evidence()
            .status_count(AdvisoryEvidenceStatus::ContractHashMissing),
        1
    );
    assert_eq!(
        selection
            .trace()
            .advisory_evidence()
            .status_count(AdvisoryEvidenceStatus::ContractHashMismatch),
        1
    );
    assert_eq!(
        selection
            .trace()
            .advisory_evidence()
            .status_count(AdvisoryEvidenceStatus::Expired),
        1
    );
    assert_eq!(
        selection
            .trace()
            .advisory_evidence()
            .status_count(AdvisoryEvidenceStatus::NotYetValid),
        1
    );

    let trace = selection.trace().as_decision_trace();
    assert_eq!(trace.decision, CriticalDecisionKind::PlanSelection);
    assert!(trace.reason.contains("catalog_version=82"));
    assert!(trace.reason.contains("stats_version=18"));
    assert!(trace.reason.contains("plan_class=StatsAdaptive"));
    assert!(trace.reason.contains("scenario_evidence_supplied=5"));
    assert!(trace.reason.contains("scenario_evidence_rejected=5"));
    assert!(trace.reason.contains("plan-class-mismatch:1"));
    assert!(trace.reason.contains("contract-hash-missing:1"));
    assert!(trace.reason.contains("contract-hash-mismatch:1"));
    assert!(trace.reason.contains("expired:1"));
    assert!(trace.reason.contains("not-yet-valid:1"));
    assert!(trace.reason.contains("advisory_only=true"));
}

#[test]
fn scenario_evidence_batch_size_is_bounded_before_plan_selection() {
    let bind = binding(951, 83, 0xAA, 19, 0xBB);
    let _key = PlanCacheKey::build(bind, PlanClass::StatsAdaptive, shaped_fingerprint())
        .expect("valid stats-adaptive key");
    let candidates = [candidate(40, PlanClass::StatsAdaptive, 100, 0x40)];

    assert_eq!(candidates.len(), 1);
    let error =
        AdvisoryEvidenceSummaryBuilder::new(PLAN_SELECTION_MAX_SCENARIO_EVIDENCE + 1).unwrap_err();
    assert_eq!(error, PlanSelectionError::TooManyScenarioEvidence);
}
