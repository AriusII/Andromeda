use andromeda_bench::{
    BenchmarkEvidence, BenchmarkEvidenceBudgets, BenchmarkEvidenceConfidence,
    BenchmarkHardwareProfile, BenchmarkHistoryAdvisoryMetadata, BenchmarkMeasurementMode,
    BenchmarkScenarioEvidence, BenchmarkScenarioEvidenceError, BenchmarkWorkloadCounter,
    BudgetStatus, DEFAULT_TEMP_BYTES, MAX_TEMP_BYTES,
};

use crate::common::{
    boundary_from_history, default_budgets, history_record_with_metrics, target, validity,
    vertical_history_record,
};

#[test]
fn boundary_rejects_history_without_enough_sample_budget() {
    let record = vertical_history_record(21);

    let error = boundary_from_history(&record, default_budgets()).unwrap_err();

    assert_eq!(
        error,
        BenchmarkScenarioEvidenceError::RecordSampleCountExceedsBudget
    );
}

#[test]
fn boundary_rejects_history_metadata_that_is_not_advisory_only() {
    let record =
        vertical_history_record(20).with_advisory_metadata(BenchmarkHistoryAdvisoryMetadata {
            advisory_boundary: "authoritative".to_string(),
            authoritative: false,
            can_select_plan_alone: false,
            optimizer_boundary: "advisory-only".to_string(),
            duration_cap_ms: 5_000,
            sample_cap: 20,
            temp_cap_bytes: DEFAULT_TEMP_BYTES,
        });

    let error = boundary_from_history(&record, default_budgets()).unwrap_err();

    assert_eq!(
        error,
        BenchmarkScenarioEvidenceError::HistoryAdvisoryBoundaryInvalid
    );

    let record =
        vertical_history_record(20).with_advisory_metadata(BenchmarkHistoryAdvisoryMetadata {
            advisory_boundary: "advisory-only".to_string(),
            authoritative: true,
            can_select_plan_alone: false,
            optimizer_boundary: "advisory-only".to_string(),
            duration_cap_ms: 5_000,
            sample_cap: 20,
            temp_cap_bytes: DEFAULT_TEMP_BYTES,
        });

    let error = boundary_from_history(&record, default_budgets()).unwrap_err();

    assert_eq!(error, BenchmarkScenarioEvidenceError::HistoryAuthoritative);
}

#[test]
fn boundary_rejects_budgets_that_exceed_history_advisory_caps() {
    let record = vertical_history_record(20).with_advisory_metadata(
        BenchmarkHistoryAdvisoryMetadata::with_resource_caps(4_000, 19, DEFAULT_TEMP_BYTES / 2)
            .unwrap(),
    );

    let duration_error = boundary_from_history(
        &record,
        BenchmarkEvidenceBudgets::new(5_000, 19, DEFAULT_TEMP_BYTES / 2).unwrap(),
    )
    .unwrap_err();
    assert_eq!(
        duration_error,
        BenchmarkScenarioEvidenceError::DurationBudgetExceedsHistoryCap
    );

    let sample_error = boundary_from_history(
        &record,
        BenchmarkEvidenceBudgets::new(4_000, 20, DEFAULT_TEMP_BYTES / 2).unwrap(),
    )
    .unwrap_err();
    assert_eq!(
        sample_error,
        BenchmarkScenarioEvidenceError::SampleBudgetExceedsHistoryCap
    );

    let temp_error = boundary_from_history(
        &record,
        BenchmarkEvidenceBudgets::new(4_000, 19, DEFAULT_TEMP_BYTES).unwrap(),
    )
    .unwrap_err();
    assert_eq!(
        temp_error,
        BenchmarkScenarioEvidenceError::TempBudgetExceedsHistoryCap
    );
}

#[test]
fn boundary_rejects_incoherent_history_metrics() {
    let impossible_error_count =
        history_record_with_metrics("vertical-v0-smoke", 15_000, 50_000, 21, 20);
    let error = boundary_from_history(&impossible_error_count, default_budgets()).unwrap_err();
    assert_eq!(
        error,
        BenchmarkScenarioEvidenceError::ErrorCountExceedsSampleCount
    );

    let inverted_percentiles =
        history_record_with_metrics("vertical-v0-smoke", 50_000, 15_000, 0, 20);
    let error = boundary_from_history(&inverted_percentiles, default_budgets()).unwrap_err();
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
        timing_source: andromeda_bench::BENCHMARK_EVIDENCE_TIMING_SOURCE_DETERMINISTIC_PLACEHOLDER,
        engine_harness: Some("MockBTreeIndex"),
        synthetic_model_version: None,
        workload_counters: vec![BenchmarkWorkloadCounter::new("lookup_operations", 5, "ops")],
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
    let record = history_record_with_metrics("inventory-reserve-stock", 15_000, 50_000, 0, 20);

    let error = boundary_from_history(&record, default_budgets()).unwrap_err();

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

    let record = history_record_with_metrics("protocol-smoke-contract", 15_000, 50_000, 0, 20);
    let error = boundary_from_history(
        &record,
        BenchmarkEvidenceBudgets::new(5_000, 20, DEFAULT_TEMP_BYTES + 1).unwrap(),
    )
    .unwrap_err();

    assert_eq!(
        error,
        BenchmarkScenarioEvidenceError::TempBudgetExceedsWorkloadLimit
    );
}
