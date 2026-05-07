use andromeda_core::{CatalogVersion, ContractHash, EngineTimestamp, ProcedureId};

use crate::{
    BenchmarkEvidence, BenchmarkHardwareProfile, BenchmarkHistoryRecord, BenchmarkMeasurementMode,
    BenchmarkWorkloadClass, MAX_EVIDENCE_TTL_MS,
};

use super::*;

fn ts(value: u64) -> EngineTimestamp {
    EngineTimestamp::from_unix_millis(value)
}

fn target() -> BenchmarkScenarioTarget {
    BenchmarkScenarioTarget::new(
        ProcedureId::new(7),
        CatalogVersion::new(11),
        ContractHash::test_vector(0xAB),
        BenchmarkStatsVersion::new(3),
        BenchmarkPlanClass::ParameterShape,
    )
    .unwrap()
}

fn validity() -> BenchmarkEvidenceValidity {
    BenchmarkEvidenceValidity::new(ts(1_000), ts(2_000)).unwrap()
}

#[test]
fn boundary_from_history_is_bounded_and_advisory() {
    let record = BenchmarkHistoryRecord::new(
        "vertical-v0-smoke".to_string(),
        "abc123".to_string(),
        "2026-05-06T12:00:00Z".to_string(),
        100,
        500,
        0,
        10,
    );
    let evidence = BenchmarkScenarioEvidence::from_history_record(
        &record,
        target(),
        BenchmarkEvidenceBudgets::new(1_000, 20, 1024 * 1024).unwrap(),
        BenchmarkEvidenceConfidence::from_permille(750).unwrap(),
        validity(),
    )
    .unwrap();

    assert!(!evidence.is_authoritative());
    assert!(!evidence.can_select_plan_alone());
    assert_eq!(evidence.optimizer_consumption_role(), "advisory-only");
    assert_eq!(evidence.workload_id(), "vertical-v0-smoke");
    assert_eq!(evidence.target().procedure_id, ProcedureId::new(7));
    assert_eq!(evidence.target().catalog_version, CatalogVersion::new(11));
    assert_eq!(
        evidence.target().stats_version,
        BenchmarkStatsVersion::new(3)
    );
    assert_eq!(
        evidence.target().plan_class,
        BenchmarkPlanClass::ParameterShape
    );
    assert_eq!(evidence.confidence().permille(), 750);
    assert_eq!(
        evidence.context().workload_class(),
        BenchmarkWorkloadClass::SyntheticDiagnostic
    );
    assert_eq!(evidence.context().hardware_profile(), None);
    assert!(evidence.validate_for_use_at(ts(1_500)).is_ok());
    assert_eq!(
        evidence.validate_for_use_at(ts(2_000)).unwrap_err(),
        BenchmarkScenarioEvidenceError::Expired
    );

    let json = evidence.to_json();
    assert!(json.contains(r#""authoritative":false"#));
    assert!(json.contains(r#""can_select_plan_alone":false"#));
    assert!(json.contains(r#""optimizer_boundary":"advisory-only""#));
    assert!(json.contains(r#""target_catalog_version":11"#));
    assert!(json.contains(r#""target_plan_class":"parameter-shape""#));
    assert!(json.contains(r#""workload_class":"synthetic-diagnostic""#));
    assert!(json.contains(r#""hardware_profile":null"#));
    assert!(json.contains(r#""confidence_permille":750"#));
    assert!(json.contains(r#""temp_budget_bytes":1048576"#));
}

#[test]
fn boundary_rejects_stale_or_untargeted_inputs() {
    assert_eq!(
        BenchmarkScenarioTarget::new(
            ProcedureId::new(0),
            CatalogVersion::new(1),
            ContractHash::test_vector(0xAB),
            BenchmarkStatsVersion::new(1),
            BenchmarkPlanClass::Singleton,
        )
        .unwrap_err(),
        BenchmarkScenarioEvidenceError::TargetProcedureIdZero
    );
    assert_eq!(
        BenchmarkScenarioTarget::new(
            ProcedureId::new(1),
            CatalogVersion::new(0),
            ContractHash::test_vector(0xAB),
            BenchmarkStatsVersion::new(1),
            BenchmarkPlanClass::Singleton,
        )
        .unwrap_err(),
        BenchmarkScenarioEvidenceError::TargetCatalogVersionZero
    );
    assert_eq!(
        BenchmarkScenarioTarget::new(
            ProcedureId::new(1),
            CatalogVersion::new(1),
            ContractHash::zero(),
            BenchmarkStatsVersion::new(1),
            BenchmarkPlanClass::Singleton,
        )
        .unwrap_err(),
        BenchmarkScenarioEvidenceError::TargetContractHashZero
    );
    assert_eq!(
        BenchmarkEvidenceConfidence::from_permille(1_001).unwrap_err(),
        BenchmarkScenarioEvidenceError::ConfidenceOutOfRange
    );
    assert_eq!(
        BenchmarkEvidenceValidity::new(ts(2_000), ts(2_000)).unwrap_err(),
        BenchmarkScenarioEvidenceError::IssuedNotBeforeExpiry
    );
}

#[test]
fn boundary_rejects_records_that_exceed_sample_budget() {
    let record = BenchmarkHistoryRecord::new(
        "wal-append-file-smoke".to_string(),
        "abc123".to_string(),
        "2026-05-06T12:00:00Z".to_string(),
        100,
        500,
        0,
        21,
    );
    let error = BenchmarkScenarioEvidence::from_history_record(
        &record,
        target(),
        BenchmarkEvidenceBudgets::new(1_000, 20, 1024 * 1024).unwrap(),
        BenchmarkEvidenceConfidence::from_permille(900).unwrap(),
        validity(),
    )
    .unwrap_err();

    assert_eq!(
        error,
        BenchmarkScenarioEvidenceError::RecordSampleCountExceedsBudget
    );
}

#[test]
fn boundary_rejects_unknown_workloads_and_workload_budget_overrides() {
    let unknown_record = BenchmarkHistoryRecord::new(
        "not-registered".to_string(),
        "abc123".to_string(),
        "2026-05-06T12:00:00Z".to_string(),
        100,
        500,
        0,
        10,
    );
    assert_eq!(
        BenchmarkScenarioEvidence::from_history_record(
            &unknown_record,
            target(),
            BenchmarkEvidenceBudgets::new(1_000, 20, 1024 * 1024).unwrap(),
            BenchmarkEvidenceConfidence::from_permille(900).unwrap(),
            validity(),
        )
        .unwrap_err(),
        BenchmarkScenarioEvidenceError::UnknownWorkloadId
    );

    let record = BenchmarkHistoryRecord::new(
        "protocol-smoke-contract".to_string(),
        "abc123".to_string(),
        "2026-05-06T12:00:00Z".to_string(),
        100,
        500,
        0,
        10,
    );
    assert_eq!(
        BenchmarkScenarioEvidence::from_history_record(
            &record,
            target(),
            BenchmarkEvidenceBudgets::new(5_001, 20, 1024 * 1024).unwrap(),
            BenchmarkEvidenceConfidence::from_permille(900).unwrap(),
            validity(),
        )
        .unwrap_err(),
        BenchmarkScenarioEvidenceError::DurationBudgetExceedsWorkloadLimit
    );
    assert_eq!(
        BenchmarkScenarioEvidence::from_history_record(
            &record,
            target(),
            BenchmarkEvidenceBudgets::new(5_000, 21, 1024 * 1024).unwrap(),
            BenchmarkEvidenceConfidence::from_permille(900).unwrap(),
            validity(),
        )
        .unwrap_err(),
        BenchmarkScenarioEvidenceError::SampleBudgetExceedsWorkloadLimit
    );
    assert_eq!(
        BenchmarkScenarioEvidence::from_history_record(
            &record,
            target(),
            BenchmarkEvidenceBudgets::new(5_000, 20, crate::DEFAULT_TEMP_BYTES + 1).unwrap(),
            BenchmarkEvidenceConfidence::from_permille(900).unwrap(),
            validity(),
        )
        .unwrap_err(),
        BenchmarkScenarioEvidenceError::TempBudgetExceedsWorkloadLimit
    );
}

#[test]
fn boundary_rejects_unbounded_expiry() {
    assert_eq!(
        BenchmarkEvidenceValidity::new(ts(1_000), ts(1_000 + MAX_EVIDENCE_TTL_MS + 1)).unwrap_err(),
        BenchmarkScenarioEvidenceError::ValidityWindowExceedsLimit
    );
}

#[test]
fn boundary_rejects_stale_target_versions() {
    let record = BenchmarkHistoryRecord::new(
        "vertical-v0-smoke".to_string(),
        "abc123".to_string(),
        "2026-05-06T12:00:00Z".to_string(),
        100,
        500,
        0,
        10,
    );
    let evidence = BenchmarkScenarioEvidence::from_history_record(
        &record,
        target(),
        BenchmarkEvidenceBudgets::new(1_000, 20, 1024 * 1024).unwrap(),
        BenchmarkEvidenceConfidence::from_permille(900).unwrap(),
        validity(),
    )
    .unwrap();

    assert!(evidence.validate_for_target_at(target(), ts(1_500)).is_ok());

    let newer_stats_target = BenchmarkScenarioTarget::new(
        ProcedureId::new(7),
        CatalogVersion::new(11),
        ContractHash::test_vector(0xAB),
        BenchmarkStatsVersion::new(4),
        BenchmarkPlanClass::ParameterShape,
    )
    .unwrap();
    assert_eq!(
        evidence
            .validate_for_target_at(newer_stats_target, ts(1_500))
            .unwrap_err(),
        BenchmarkScenarioEvidenceError::TargetStatsVersionMismatch
    );
}

#[test]
fn boundary_from_runtime_evidence_carries_profile_context() {
    let evidence = BenchmarkEvidence {
        workload_id: "protocol-smoke-contract".to_string(),
        workload_hypothesis: "typed protocol contract inspection should stay bounded without opening a network surface",
        workload_shape_version: "protocol-smoke-contract.synthetic.v1",
        workload_size: "contract-only synthetic inspection, samples<=20, duration_ms<=5000",
        primary_metric: "p50_latency_us,p95_latency_us,error_rate_ppm",
        baseline_ref: "history.protocol-smoke-contract.synthetic.v1",
        budget_origin: "static-workload-registry-v1",
        decision_linkage: "advisory-only; requires ProcedureId+CatalogVersion+ContractHash+StatsVersion+PlanClass",
        hardware_profile: BenchmarkHardwareProfile::DeclaredLocal,
        duration_ms: 1_000,
        samples: 5,
        warmups: 1,
        temp_budget_bytes: 1024 * 1024,
        started_at_unix_ms: 1,
        elapsed_ms: 6,
        sample_count: 5,
        p50_latency_us: 100,
        p95_latency_us: 200,
        error_count: 0,
        budget_status: crate::BudgetStatus::Passed,
        diagnostic_only: true,
        measurement_mode: BenchmarkMeasurementMode::SyntheticDiagnostic,
        latency_source: "deterministic-latency-model",
        timing_source: crate::BENCHMARK_EVIDENCE_TIMING_SOURCE_DETERMINISTIC_PLACEHOLDER,
        engine_harness: None,
        synthetic_model_version: Some("bounded-diagnostic-v1"),
        workload_counters: vec![crate::BenchmarkWorkloadCounter::new(
            "requested_samples",
            5,
            "samples",
        )],
    };

    let boundary = BenchmarkScenarioEvidence::from_benchmark_evidence(
        &evidence,
        "abc123",
        "2026-05-06T12:00:00Z",
        target(),
        BenchmarkEvidenceConfidence::from_permille(900).unwrap(),
        validity(),
    )
    .unwrap();

    assert_eq!(
        boundary.context().hardware_profile(),
        Some(BenchmarkHardwareProfile::DeclaredLocal)
    );
    assert_eq!(
        boundary.context().measurement_mode(),
        Some(BenchmarkMeasurementMode::SyntheticDiagnostic)
    );
    assert_eq!(
        boundary.context().timing_source(),
        Some(crate::BENCHMARK_EVIDENCE_TIMING_SOURCE_DETERMINISTIC_PLACEHOLDER)
    );
    assert_eq!(
        boundary.context().synthetic_model_version(),
        Some("bounded-diagnostic-v1")
    );
    assert_eq!(boundary.context().engine_harness(), None);
    let json = boundary.to_json();
    assert!(json.contains(r#""hardware_profile":"declared-local""#));
    assert!(json.contains(r#""measurement_mode":"synthetic-diagnostic""#));
    assert!(json.contains(r#""timing_source":"deterministic-run-clock-placeholder""#));
    assert!(json.contains(r#""synthetic_model_version":"bounded-diagnostic-v1""#));
}

#[test]
fn boundary_rejects_incoherent_runtime_context() {
    let evidence = BenchmarkEvidence {
        workload_id: "btree-lookup-smoke".to_string(),
        workload_hypothesis: "mock B-Tree single-key lookup latency should remain stable over the bounded read-only key set",
        workload_shape_version: "btree-lookup-smoke.harness.v1",
        workload_size: "mock read-only key set, single-key lookup, samples<=20",
        primary_metric: "p50_latency_us,p95_latency_us,error_rate_ppm",
        baseline_ref: "history.btree-lookup-smoke.harness.v1",
        budget_origin: "static-workload-registry-v1",
        decision_linkage: "advisory-only; requires ProcedureId+CatalogVersion+ContractHash+StatsVersion+PlanClass",
        hardware_profile: BenchmarkHardwareProfile::Conservative,
        duration_ms: 1_000,
        samples: 5,
        warmups: 0,
        temp_budget_bytes: 1024 * 1024,
        started_at_unix_ms: 1,
        elapsed_ms: 5,
        sample_count: 5,
        p50_latency_us: 100,
        p95_latency_us: 200,
        error_count: 0,
        budget_status: crate::BudgetStatus::Passed,
        diagnostic_only: true,
        measurement_mode: BenchmarkMeasurementMode::HarnessDiagnostic,
        latency_source: "in-memory-btree-read-harness",
        timing_source: crate::BENCHMARK_EVIDENCE_TIMING_SOURCE_DETERMINISTIC_PLACEHOLDER,
        engine_harness: None,
        synthetic_model_version: None,
        workload_counters: vec![crate::BenchmarkWorkloadCounter::new(
            "lookup_operations",
            5,
            "ops",
        )],
    };

    assert_eq!(
        BenchmarkScenarioEvidence::from_benchmark_evidence(
            &evidence,
            "abc123",
            "2026-05-06T12:00:00Z",
            target(),
            BenchmarkEvidenceConfidence::from_permille(900).unwrap(),
            validity(),
        )
        .unwrap_err(),
        BenchmarkScenarioEvidenceError::MissingEngineHarness
    );
}

#[test]
fn plan_class_tags_match_the_catalog_boundary_contract() {
    assert_eq!(BenchmarkPlanClass::VARIANT_COUNT, 4);
    assert_eq!(BenchmarkPlanClass::Singleton.as_tag(), 0x01);
    assert_eq!(BenchmarkPlanClass::ParameterShape.as_tag(), 0x02);
    assert_eq!(BenchmarkPlanClass::Cardinality.as_tag(), 0x03);
    assert_eq!(BenchmarkPlanClass::StatsAdaptive.as_tag(), 0x04);
}
