use super::*;

#[test]
fn minimal_plan_selection_keeps_scenario_evidence_advisory_only() {
    let bind = binding(800, 60, 0xAA, 12, 0xBB);
    let key = PlanCacheKey::build(&bind, PlanClass::StatsAdaptive, shaped_fingerprint())
        .expect("valid stats-adaptive key");
    let candidates = [
        candidate(20, PlanClass::StatsAdaptive, 400, 0x20),
        candidate(10, PlanClass::StatsAdaptive, 100, 0x10),
        candidate(30, PlanClass::Cardinality, 0, 0x30),
    ];
    let evidence = scenario_evidence_for_key(key, 1, 1_000, 1_000);

    assert!(!evidence.is_authoritative());
    assert_eq!(
        classify_advisory_evidence_for_key(&key, &evidence, ts(150)),
        AdvisoryEvidenceStatus::AcceptedAdvisory
    );

    let selection =
        select_minimal_plan(key, &candidates, &[evidence], ts(150), TraceId::new(16_001))
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
    let key = PlanCacheKey::build(&bind, PlanClass::ParameterShape, shaped_fingerprint())
        .expect("valid parameter-shape key");
    let candidates = [
        candidate(11, PlanClass::ParameterShape, 200, 0x11),
        candidate(12, PlanClass::ParameterShape, 300, 0x12),
    ];
    let mut stale_target_evidence = scenario_evidence_for_key(key, 2, 1_000, 1_000);
    stale_target_evidence = ScenarioEvidence::new(
        stale_target_evidence.scenario_id(),
        stale_target_evidence.kind(),
        ScenarioTarget {
            procedure_id: key.procedure_id,
            catalog_version: key.catalog_version,
            stats_version: StatsVersion::new(key.stats_version.get() + 1),
            plan_class: Some(key.plan_class),
            contract_hash: Some(key.contract_hash),
        },
        stale_target_evidence.score(),
        stale_target_evidence.confidence(),
        stale_target_evidence.validity(),
    )
    .expect("stale target is structurally valid evidence");

    assert_eq!(
        classify_advisory_evidence_for_key(&key, &stale_target_evidence, ts(150)),
        AdvisoryEvidenceStatus::StatsVersionMismatch
    );

    let selection = select_minimal_plan(
        key,
        &candidates,
        &[stale_target_evidence],
        ts(150),
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
    let key = PlanCacheKey::build(&bind, PlanClass::ParameterShape, shaped_fingerprint())
        .expect("valid parameter-shape key");
    let candidates = [
        candidate(21, PlanClass::ParameterShape, 200, 0x21),
        candidate(22, PlanClass::ParameterShape, 300, 0x22),
    ];
    let mut stale_catalog_evidence = scenario_evidence_for_key(key, 3, 1_000, 1_000);
    stale_catalog_evidence = ScenarioEvidence::new(
        stale_catalog_evidence.scenario_id(),
        stale_catalog_evidence.kind(),
        ScenarioTarget {
            procedure_id: key.procedure_id,
            catalog_version: CatalogVersion::new(key.catalog_version.get() + 1),
            stats_version: key.stats_version,
            plan_class: Some(key.plan_class),
            contract_hash: Some(key.contract_hash),
        },
        stale_catalog_evidence.score(),
        stale_catalog_evidence.confidence(),
        stale_catalog_evidence.validity(),
    )
    .expect("stale catalog target is structurally valid evidence");

    assert_eq!(
        classify_advisory_evidence_for_key(&key, &stale_catalog_evidence, ts(150)),
        AdvisoryEvidenceStatus::CatalogVersionMismatch
    );

    let selection = select_minimal_plan(
        key,
        &candidates,
        &[stale_catalog_evidence],
        ts(150),
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
    let key = PlanCacheKey::build(&bind, PlanClass::StatsAdaptive, shaped_fingerprint())
        .expect("valid stats-adaptive key");
    let candidates = [
        candidate(30, PlanClass::StatsAdaptive, 100, 0x30),
        candidate(31, PlanClass::StatsAdaptive, 900, 0x31),
    ];

    let plan_class_mismatch = ScenarioEvidence::new(
        ScenarioId::new(30).unwrap(),
        ScenarioKind::Microbenchmark,
        ScenarioTarget {
            procedure_id: key.procedure_id,
            catalog_version: key.catalog_version,
            stats_version: key.stats_version,
            plan_class: Some(PlanClass::Cardinality),
            contract_hash: Some(key.contract_hash),
        },
        EvidenceScore::from_permille(1_000).unwrap(),
        EvidenceConfidence::from_permille(1_000).unwrap(),
        ValidityWindow::new(ts(100), ts(200)).unwrap(),
    )
    .unwrap();
    let missing_contract = ScenarioEvidence::new(
        ScenarioId::new(31).unwrap(),
        ScenarioKind::Microbenchmark,
        ScenarioTarget {
            procedure_id: key.procedure_id,
            catalog_version: key.catalog_version,
            stats_version: key.stats_version,
            plan_class: Some(key.plan_class),
            contract_hash: None,
        },
        EvidenceScore::from_permille(1_000).unwrap(),
        EvidenceConfidence::from_permille(1_000).unwrap(),
        ValidityWindow::new(ts(100), ts(200)).unwrap(),
    )
    .unwrap();
    let contract_mismatch = ScenarioEvidence::new(
        ScenarioId::new(32).unwrap(),
        ScenarioKind::Microbenchmark,
        ScenarioTarget {
            procedure_id: key.procedure_id,
            catalog_version: key.catalog_version,
            stats_version: key.stats_version,
            plan_class: Some(key.plan_class),
            contract_hash: Some(ContractHash::new([0xCC; ContractHash::LEN])),
        },
        EvidenceScore::from_permille(1_000).unwrap(),
        EvidenceConfidence::from_permille(1_000).unwrap(),
        ValidityWindow::new(ts(100), ts(200)).unwrap(),
    )
    .unwrap();
    let expired = ScenarioEvidence::new(
        ScenarioId::new(33).unwrap(),
        ScenarioKind::Microbenchmark,
        ScenarioTarget {
            procedure_id: key.procedure_id,
            catalog_version: key.catalog_version,
            stats_version: key.stats_version,
            plan_class: Some(key.plan_class),
            contract_hash: Some(key.contract_hash),
        },
        EvidenceScore::from_permille(1_000).unwrap(),
        EvidenceConfidence::from_permille(1_000).unwrap(),
        ValidityWindow::new(ts(100), ts(120)).unwrap(),
    )
    .unwrap();
    let not_yet_valid = ScenarioEvidence::new(
        ScenarioId::new(34).unwrap(),
        ScenarioKind::Microbenchmark,
        ScenarioTarget {
            procedure_id: key.procedure_id,
            catalog_version: key.catalog_version,
            stats_version: key.stats_version,
            plan_class: Some(key.plan_class),
            contract_hash: Some(key.contract_hash),
        },
        EvidenceScore::from_permille(1_000).unwrap(),
        EvidenceConfidence::from_permille(1_000).unwrap(),
        ValidityWindow::new(ts(180), ts(220)).unwrap(),
    )
    .unwrap();

    assert_eq!(
        classify_advisory_evidence_for_key(&key, &plan_class_mismatch, ts(150)),
        AdvisoryEvidenceStatus::PlanClassMismatch
    );
    assert_eq!(
        classify_advisory_evidence_for_key(&key, &missing_contract, ts(150)),
        AdvisoryEvidenceStatus::ContractHashMissing
    );
    assert_eq!(
        classify_advisory_evidence_for_key(&key, &contract_mismatch, ts(150)),
        AdvisoryEvidenceStatus::ContractHashMismatch
    );
    assert_eq!(
        classify_advisory_evidence_for_key(&key, &expired, ts(150)),
        AdvisoryEvidenceStatus::Expired
    );
    assert_eq!(
        classify_advisory_evidence_for_key(&key, &not_yet_valid, ts(150)),
        AdvisoryEvidenceStatus::NotYetValid
    );

    let selection = select_minimal_plan(
        key,
        &candidates,
        &[
            plan_class_mismatch,
            missing_contract,
            contract_mismatch,
            expired,
            not_yet_valid,
        ],
        ts(150),
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
    let key = PlanCacheKey::build(&bind, PlanClass::StatsAdaptive, shaped_fingerprint())
        .expect("valid stats-adaptive key");
    let candidates = [candidate(40, PlanClass::StatsAdaptive, 100, 0x40)];

    let mut evidence = Vec::new();
    for index in 0..=PLAN_SELECTION_MAX_SCENARIO_EVIDENCE {
        evidence.push(scenario_evidence_for_key(key, 40 + index as u64, 900, 900));
    }

    let error = select_minimal_plan(key, &candidates, &evidence, ts(150), TraceId::new(16_014))
        .unwrap_err();
    assert_eq!(error, PlanSelectionError::TooManyScenarioEvidence);
}
