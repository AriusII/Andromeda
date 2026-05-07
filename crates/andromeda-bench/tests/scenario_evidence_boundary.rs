#![forbid(unsafe_code)]

//! Benchmark history to ScenarioEvidence boundary integration tests.

use andromeda_bench::benchmark_history::BenchmarkHistoryAdvisoryMetadata;
use andromeda_bench::{
    BenchmarkEvidence, BenchmarkEvidenceBudgets, BenchmarkEvidenceConfidence,
    BenchmarkEvidenceValidity, BenchmarkHardwareProfile, BenchmarkHistoryRecord,
    BenchmarkMeasurementMode, BenchmarkPlanClass, BenchmarkRunRequest, BenchmarkScenarioEvidence,
    BenchmarkScenarioEvidenceError, BenchmarkScenarioTarget, BenchmarkStatsVersion,
    BenchmarkWorkloadClass, BudgetStatus, DEFAULT_TEMP_BYTES, MAX_TEMP_BYTES,
    run_bounded_benchmark,
};
use andromeda_core::{CatalogVersion, ContractHash, EngineTimestamp, ProcedureId};

fn ts(value: u64) -> EngineTimestamp {
    EngineTimestamp::from_unix_millis(value)
}

fn target() -> BenchmarkScenarioTarget {
    target_with_stats(1)
}

fn target_with_stats(stats_version: u64) -> BenchmarkScenarioTarget {
    BenchmarkScenarioTarget::new(
        ProcedureId::new(0x5253),
        CatalogVersion::new(9),
        ContractHash::test_vector(0x52),
        BenchmarkStatsVersion::new(stats_version),
        BenchmarkPlanClass::StatsAdaptive,
    )
    .unwrap()
}

fn validity() -> BenchmarkEvidenceValidity {
    BenchmarkEvidenceValidity::new(ts(1_000), ts(86_401_000)).unwrap()
}

#[test]
fn history_record_exports_non_authoritative_scenario_boundary() {
    let record = BenchmarkHistoryRecord::new(
        "vertical-v0-smoke".to_string(),
        "commit-20260506".to_string(),
        "2026-05-06T10:00:00Z".to_string(),
        15_000,
        50_000,
        0,
        20,
    );

    let boundary = record
        .to_scenario_evidence_boundary(
            target(),
            BenchmarkEvidenceBudgets::new(5_000, 20, DEFAULT_TEMP_BYTES).unwrap(),
            BenchmarkEvidenceConfidence::from_permille(850).unwrap(),
            validity(),
        )
        .unwrap();

    assert!(!boundary.is_authoritative());
    assert!(!boundary.can_select_plan_alone());
    assert_eq!(boundary.workload_id(), "vertical-v0-smoke");
    assert_eq!(boundary.commit_id(), "commit-20260506");
    assert_eq!(boundary.target().procedure_id, ProcedureId::new(0x5253));
    assert_eq!(boundary.target().catalog_version, CatalogVersion::new(9));
    assert_eq!(
        boundary.target().contract_hash,
        ContractHash::test_vector(0x52)
    );
    assert_eq!(
        boundary.target().stats_version,
        BenchmarkStatsVersion::new(1)
    );
    assert_eq!(
        boundary.target().plan_class,
        BenchmarkPlanClass::StatsAdaptive
    );
    assert_eq!(boundary.budgets().duration_ms, 5_000);
    assert_eq!(boundary.budgets().samples, 20);
    assert_eq!(boundary.budgets().temp_bytes, DEFAULT_TEMP_BYTES);
    assert_eq!(boundary.confidence().permille(), 850);
    assert_eq!(
        boundary.context().workload_class(),
        BenchmarkWorkloadClass::SyntheticDiagnostic
    );
    assert_eq!(boundary.context().hardware_profile(), None);
    assert!(boundary.validate_for_use_at(ts(2_000)).is_ok());

    let json = boundary.to_json();
    assert!(json.contains(r#""authoritative":false"#));
    assert!(json.contains(r#""can_select_plan_alone":false"#));
    assert!(json.contains(r#""optimizer_boundary":"advisory-only""#));
    assert!(json.contains(r#""target_plan_class":"stats-adaptive""#));
    assert!(json.contains(r#""target_procedure_id":21075"#));
    assert!(json.contains(r#""target_catalog_version":9"#));
    assert!(json.contains(r#""target_stats_version":1"#));
    assert!(json.contains(r#""hardware_profile":null"#));
}

#[test]
fn benchmark_run_evidence_preserves_explicit_generation_budgets() {
    let mut request = BenchmarkRunRequest::new("protocol-smoke-contract");
    request.duration_ms = 1_000;
    request.samples = 5;
    request.warmups = 1;
    request.temp_budget_bytes = DEFAULT_TEMP_BYTES / 2;

    let evidence = run_bounded_benchmark(&request).unwrap();
    let boundary = BenchmarkScenarioEvidence::from_benchmark_evidence(
        &evidence,
        "commit-20260506",
        "2026-05-06T10:00:00Z",
        target(),
        BenchmarkEvidenceConfidence::from_permille(700).unwrap(),
        validity(),
    )
    .unwrap();

    assert_eq!(boundary.sample_count(), 5);
    assert_eq!(boundary.budgets().duration_ms, 1_000);
    assert_eq!(boundary.budgets().samples, 5);
    assert_eq!(boundary.budgets().temp_bytes, DEFAULT_TEMP_BYTES / 2);
    assert_eq!(
        boundary.context().hardware_profile(),
        Some(request.hardware_profile)
    );
    assert_eq!(
        boundary.context().measurement_mode(),
        Some(BenchmarkMeasurementMode::SyntheticDiagnostic)
    );
    assert_eq!(
        boundary.context().latency_source(),
        Some("deterministic-latency-model")
    );
    assert!(!boundary.is_authoritative());
}

#[test]
fn boundary_rejects_history_without_enough_sample_budget() {
    let record = BenchmarkHistoryRecord::new(
        "vertical-v0-smoke".to_string(),
        "commit-20260506".to_string(),
        "2026-05-06T10:00:00Z".to_string(),
        15_000,
        50_000,
        0,
        21,
    );

    let error = record
        .to_scenario_evidence_boundary(
            target(),
            BenchmarkEvidenceBudgets::new(5_000, 20, DEFAULT_TEMP_BYTES).unwrap(),
            BenchmarkEvidenceConfidence::from_permille(850).unwrap(),
            validity(),
        )
        .unwrap_err();

    assert_eq!(
        error,
        BenchmarkScenarioEvidenceError::RecordSampleCountExceedsBudget
    );
}

#[test]
fn boundary_rejects_history_metadata_that_is_not_advisory_only() {
    let record = BenchmarkHistoryRecord::new(
        "vertical-v0-smoke".to_string(),
        "commit-20260506".to_string(),
        "2026-05-06T10:00:00Z".to_string(),
        15_000,
        50_000,
        0,
        20,
    )
    .with_advisory_metadata(BenchmarkHistoryAdvisoryMetadata {
        advisory_boundary: "authoritative".to_string(),
        authoritative: false,
        can_select_plan_alone: false,
        optimizer_boundary: "advisory-only".to_string(),
        duration_cap_ms: 5_000,
        sample_cap: 20,
        temp_cap_bytes: DEFAULT_TEMP_BYTES,
    });

    let error = record
        .to_scenario_evidence_boundary(
            target(),
            BenchmarkEvidenceBudgets::new(5_000, 20, DEFAULT_TEMP_BYTES).unwrap(),
            BenchmarkEvidenceConfidence::from_permille(850).unwrap(),
            validity(),
        )
        .unwrap_err();

    assert_eq!(
        error,
        BenchmarkScenarioEvidenceError::HistoryAdvisoryBoundaryInvalid
    );

    let record = BenchmarkHistoryRecord::new(
        "vertical-v0-smoke".to_string(),
        "commit-20260506".to_string(),
        "2026-05-06T10:00:00Z".to_string(),
        15_000,
        50_000,
        0,
        20,
    )
    .with_advisory_metadata(BenchmarkHistoryAdvisoryMetadata {
        advisory_boundary: "advisory-only".to_string(),
        authoritative: true,
        can_select_plan_alone: false,
        optimizer_boundary: "advisory-only".to_string(),
        duration_cap_ms: 5_000,
        sample_cap: 20,
        temp_cap_bytes: DEFAULT_TEMP_BYTES,
    });

    let error = record
        .to_scenario_evidence_boundary(
            target(),
            BenchmarkEvidenceBudgets::new(5_000, 20, DEFAULT_TEMP_BYTES).unwrap(),
            BenchmarkEvidenceConfidence::from_permille(850).unwrap(),
            validity(),
        )
        .unwrap_err();

    assert_eq!(error, BenchmarkScenarioEvidenceError::HistoryAuthoritative);
}

#[test]
fn boundary_rejects_budgets_that_exceed_history_advisory_caps() {
    let record = BenchmarkHistoryRecord::new(
        "vertical-v0-smoke".to_string(),
        "commit-20260506".to_string(),
        "2026-05-06T10:00:00Z".to_string(),
        15_000,
        50_000,
        0,
        20,
    )
    .with_advisory_metadata(
        BenchmarkHistoryAdvisoryMetadata::with_resource_caps(4_000, 19, DEFAULT_TEMP_BYTES / 2)
            .unwrap(),
    );

    let duration_error = record
        .to_scenario_evidence_boundary(
            target(),
            BenchmarkEvidenceBudgets::new(5_000, 19, DEFAULT_TEMP_BYTES / 2).unwrap(),
            BenchmarkEvidenceConfidence::from_permille(850).unwrap(),
            validity(),
        )
        .unwrap_err();
    assert_eq!(
        duration_error,
        BenchmarkScenarioEvidenceError::DurationBudgetExceedsHistoryCap
    );

    let sample_error = record
        .to_scenario_evidence_boundary(
            target(),
            BenchmarkEvidenceBudgets::new(4_000, 20, DEFAULT_TEMP_BYTES / 2).unwrap(),
            BenchmarkEvidenceConfidence::from_permille(850).unwrap(),
            validity(),
        )
        .unwrap_err();
    assert_eq!(
        sample_error,
        BenchmarkScenarioEvidenceError::SampleBudgetExceedsHistoryCap
    );

    let temp_error = record
        .to_scenario_evidence_boundary(
            target(),
            BenchmarkEvidenceBudgets::new(4_000, 19, DEFAULT_TEMP_BYTES).unwrap(),
            BenchmarkEvidenceConfidence::from_permille(850).unwrap(),
            validity(),
        )
        .unwrap_err();
    assert_eq!(
        temp_error,
        BenchmarkScenarioEvidenceError::TempBudgetExceedsHistoryCap
    );
}

#[test]
fn boundary_rejects_incoherent_history_metrics() {
    let impossible_error_count = BenchmarkHistoryRecord::new(
        "vertical-v0-smoke".to_string(),
        "commit-20260506".to_string(),
        "2026-05-06T10:00:00Z".to_string(),
        15_000,
        50_000,
        21,
        20,
    );
    let error = impossible_error_count
        .to_scenario_evidence_boundary(
            target(),
            BenchmarkEvidenceBudgets::new(5_000, 20, DEFAULT_TEMP_BYTES).unwrap(),
            BenchmarkEvidenceConfidence::from_permille(850).unwrap(),
            validity(),
        )
        .unwrap_err();
    assert_eq!(
        error,
        BenchmarkScenarioEvidenceError::ErrorCountExceedsSampleCount
    );

    let inverted_percentiles = BenchmarkHistoryRecord::new(
        "vertical-v0-smoke".to_string(),
        "commit-20260506".to_string(),
        "2026-05-06T10:00:00Z".to_string(),
        50_000,
        15_000,
        0,
        20,
    );
    let error = inverted_percentiles
        .to_scenario_evidence_boundary(
            target(),
            BenchmarkEvidenceBudgets::new(5_000, 20, DEFAULT_TEMP_BYTES).unwrap(),
            BenchmarkEvidenceConfidence::from_permille(850).unwrap(),
            validity(),
        )
        .unwrap_err();
    assert_eq!(
        error,
        BenchmarkScenarioEvidenceError::LatencyPercentileOrderInvalid
    );
}

#[test]
fn boundary_rejects_workload_measurement_mode_mismatch() {
    let evidence = BenchmarkEvidence {
        workload_id: "protocol-smoke-contract".to_string(),
        workload_hypothesis: "typed protocol contract inspection should stay bounded without opening a network surface",
        workload_shape_version: "protocol-smoke-contract.synthetic.v1",
        workload_size: "contract-only synthetic inspection, samples<=20, duration_ms<=5000",
        primary_metric: "p50_latency_us,p95_latency_us,error_rate_ppm",
        baseline_ref: "history.protocol-smoke-contract.synthetic.v1",
        budget_origin: "static-workload-registry-v1",
        decision_linkage: "advisory-only; requires ProcedureId+CatalogVersion+ContractHash+StatsVersion+PlanClass",
        hardware_profile: BenchmarkHardwareProfile::Conservative,
        duration_ms: 1_000,
        samples: 5,
        warmups: 0,
        temp_budget_bytes: DEFAULT_TEMP_BYTES / 2,
        started_at_unix_ms: 1,
        elapsed_ms: 5,
        sample_count: 5,
        p50_latency_us: 100,
        p95_latency_us: 200,
        error_count: 0,
        budget_status: BudgetStatus::Passed,
        diagnostic_only: true,
        measurement_mode: BenchmarkMeasurementMode::HarnessDiagnostic,
        latency_source: "in-memory-btree-read-harness",
        engine_harness: Some("MockBTreeIndex"),
        synthetic_model_version: None,
    };

    let error = BenchmarkScenarioEvidence::from_benchmark_evidence(
        &evidence,
        "commit-20260506",
        "2026-05-06T10:00:00Z",
        target(),
        BenchmarkEvidenceConfidence::from_permille(700).unwrap(),
        validity(),
    )
    .unwrap_err();

    assert_eq!(
        error,
        BenchmarkScenarioEvidenceError::WorkloadMeasurementModeMismatch
    );
}

#[test]
fn boundary_requires_registered_workload_identity() {
    let record = BenchmarkHistoryRecord::new(
        "inventory-reserve-stock".to_string(),
        "commit-20260506".to_string(),
        "2026-05-06T10:00:00Z".to_string(),
        15_000,
        50_000,
        0,
        20,
    );

    let error = record
        .to_scenario_evidence_boundary(
            target(),
            BenchmarkEvidenceBudgets::new(5_000, 20, DEFAULT_TEMP_BYTES).unwrap(),
            BenchmarkEvidenceConfidence::from_permille(850).unwrap(),
            validity(),
        )
        .unwrap_err();

    assert_eq!(error, BenchmarkScenarioEvidenceError::UnknownWorkloadId);
}

#[test]
fn boundary_rejects_zero_global_and_workload_temp_budget_overruns() {
    assert_eq!(
        BenchmarkEvidenceBudgets::new(1_000, 10, 0).unwrap_err(),
        BenchmarkScenarioEvidenceError::ZeroTempBudget
    );
    assert_eq!(
        BenchmarkEvidenceBudgets::new(1_000, 10, MAX_TEMP_BYTES + 1).unwrap_err(),
        BenchmarkScenarioEvidenceError::TempBudgetExceedsGlobalLimit
    );

    let record = BenchmarkHistoryRecord::new(
        "protocol-smoke-contract".to_string(),
        "commit-20260506".to_string(),
        "2026-05-06T10:00:00Z".to_string(),
        15_000,
        50_000,
        0,
        20,
    );
    let error = record
        .to_scenario_evidence_boundary(
            target(),
            BenchmarkEvidenceBudgets::new(5_000, 20, DEFAULT_TEMP_BYTES + 1).unwrap(),
            BenchmarkEvidenceConfidence::from_permille(850).unwrap(),
            validity(),
        )
        .unwrap_err();

    assert_eq!(
        error,
        BenchmarkScenarioEvidenceError::TempBudgetExceedsWorkloadLimit
    );
}

#[test]
fn boundary_rejects_missing_contract_hash_and_stale_stats_version() {
    assert_eq!(
        BenchmarkScenarioTarget::new(
            ProcedureId::new(0x5253),
            CatalogVersion::new(9),
            ContractHash::zero(),
            BenchmarkStatsVersion::new(1),
            BenchmarkPlanClass::StatsAdaptive,
        )
        .unwrap_err(),
        BenchmarkScenarioEvidenceError::TargetContractHashZero
    );

    let record = BenchmarkHistoryRecord::new(
        "vertical-v0-smoke".to_string(),
        "commit-20260506".to_string(),
        "2026-05-06T10:00:00Z".to_string(),
        15_000,
        50_000,
        0,
        20,
    );
    let boundary = record
        .to_scenario_evidence_boundary(
            target(),
            BenchmarkEvidenceBudgets::new(5_000, 20, DEFAULT_TEMP_BYTES).unwrap(),
            BenchmarkEvidenceConfidence::from_permille(850).unwrap(),
            validity(),
        )
        .unwrap();

    assert_eq!(
        boundary
            .validate_for_target_at(target_with_stats(2), ts(2_000))
            .unwrap_err(),
        BenchmarkScenarioEvidenceError::TargetStatsVersionMismatch
    );
}

#[test]
fn boundary_validity_window_is_half_open_and_expirable() {
    let record = BenchmarkHistoryRecord::new(
        "vertical-v0-smoke".to_string(),
        "commit-20260506".to_string(),
        "2026-05-06T10:00:00Z".to_string(),
        15_000,
        50_000,
        0,
        20,
    );
    let short_validity = BenchmarkEvidenceValidity::new(ts(1_000), ts(2_000)).unwrap();
    let boundary = record
        .to_scenario_evidence_boundary(
            target(),
            BenchmarkEvidenceBudgets::new(5_000, 20, DEFAULT_TEMP_BYTES).unwrap(),
            BenchmarkEvidenceConfidence::from_permille(850).unwrap(),
            short_validity,
        )
        .unwrap();

    assert_eq!(
        boundary.validate_for_use_at(ts(999)).unwrap_err(),
        BenchmarkScenarioEvidenceError::NotYetValid
    );
    assert!(boundary.validate_for_use_at(ts(1_000)).is_ok());
    assert_eq!(
        boundary
            .validate_for_target_at(target(), ts(2_000))
            .unwrap_err(),
        BenchmarkScenarioEvidenceError::Expired
    );
}

#[test]
fn boundary_rejects_unversioned_or_unbounded_plan_cache_inputs() {
    let target_error = BenchmarkScenarioTarget::new(
        ProcedureId::new(0x5253),
        CatalogVersion::new(0),
        ContractHash::test_vector(0x52),
        BenchmarkStatsVersion::new(1),
        BenchmarkPlanClass::StatsAdaptive,
    )
    .unwrap_err();
    assert_eq!(
        target_error,
        BenchmarkScenarioEvidenceError::TargetCatalogVersionZero
    );

    let stats_error = BenchmarkScenarioTarget::new(
        ProcedureId::new(0x5253),
        CatalogVersion::new(9),
        ContractHash::test_vector(0x52),
        BenchmarkStatsVersion::new(0),
        BenchmarkPlanClass::StatsAdaptive,
    )
    .unwrap_err();
    assert_eq!(
        stats_error,
        BenchmarkScenarioEvidenceError::TargetStatsVersionZero
    );

    assert_eq!(
        BenchmarkEvidenceBudgets::new(5_000, 20, 0).unwrap_err(),
        BenchmarkScenarioEvidenceError::ZeroTempBudget
    );
}

#[test]
fn boundary_validity_window_is_half_open_and_explicit() {
    let record = BenchmarkHistoryRecord::new(
        "vertical-v0-smoke".to_string(),
        "commit-20260506".to_string(),
        "2026-05-06T10:00:00Z".to_string(),
        15_000,
        50_000,
        0,
        20,
    );

    let boundary = record
        .to_scenario_evidence_boundary(
            target(),
            BenchmarkEvidenceBudgets::new(5_000, 20, DEFAULT_TEMP_BYTES).unwrap(),
            BenchmarkEvidenceConfidence::from_permille(850).unwrap(),
            validity(),
        )
        .unwrap();

    assert_eq!(
        boundary.validate_for_use_at(ts(999)).unwrap_err(),
        BenchmarkScenarioEvidenceError::NotYetValid
    );
    assert!(boundary.validate_for_use_at(ts(1_000)).is_ok());
    assert_eq!(
        boundary.validate_for_use_at(ts(86_401_000)).unwrap_err(),
        BenchmarkScenarioEvidenceError::Expired
    );
}
