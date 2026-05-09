use super::*;
use crate::metric_math::percent_change;
use andromeda_bench_workload::{BenchmarkHardwareProfile, BenchmarkRunRequest, BudgetStatus};
use andromeda_scenario_evidence::{
    BENCHMARK_EVIDENCE_TIMING_SOURCE_DETERMINISTIC_PLACEHOLDER, BenchmarkEvidence,
    BenchmarkMeasurementMode, BenchmarkPlanClass, BenchmarkScenarioTarget, BenchmarkStatsVersion,
};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

fn target(stats_version: u64) -> BenchmarkScenarioTarget {
    BenchmarkScenarioTarget::new(
        ProcedureId::new(0xBEEF),
        CatalogVersion::new(9),
        ContractHash::test_vector(0xA5),
        BenchmarkStatsVersion::new(stats_version),
        BenchmarkPlanClass::StatsAdaptive,
    )
    .unwrap()
}

fn evidence_from_request(request: &BenchmarkRunRequest) -> BenchmarkEvidence {
    let workload = request.validate().unwrap();
    BenchmarkEvidence {
        workload_id: workload.id.to_string(),
        workload_hypothesis: workload.hypothesis,
        workload_shape_version: workload.workload_shape_version,
        workload_size: workload.workload_size,
        primary_metric: workload.primary_metric,
        baseline_ref: workload.baseline_ref,
        budget_origin: workload.budget_origin,
        decision_linkage: workload.decision_linkage,
        hardware_profile: request.hardware_profile,
        duration_ms: request.duration_ms,
        samples: request.samples,
        warmups: request.warmups,
        temp_budget_bytes: request.temp_budget_bytes,
        started_at_unix_ms: 0,
        elapsed_ms: request.duration_ms,
        sample_count: request.samples,
        p50_latency_us: 10_000,
        p95_latency_us: 50_000,
        error_count: 0,
        budget_status: BudgetStatus::Passed,
        diagnostic_only: true,
        measurement_mode: BenchmarkMeasurementMode::SyntheticDiagnostic,
        latency_source: "regression-test-fixture",
        timing_source: BENCHMARK_EVIDENCE_TIMING_SOURCE_DETERMINISTIC_PLACEHOLDER,
        engine_harness: None,
        synthetic_model_version: Some("regression-test.synthetic.v1"),
        workload_counters: Vec::new(),
    }
}

#[test]
fn baseline_creation_and_json_serialization() {
    let baseline = BenchmarkBaseline::from_evidence(
        "protocol-smoke-contract".to_string(),
        10_000,
        50_000,
        0,
        20,
        "2026-01-15T14:30:45Z".to_string(),
    );

    let json = baseline.to_json();
    let recovered = BenchmarkBaseline::from_json(&json).unwrap();

    assert_eq!(baseline, recovered);
    assert!(json.contains("\"workload_id\":\"protocol-smoke-contract\""));
    assert!(json.contains("\"p50_latency_us\":10000"));
    assert!(json.contains("\"workload_shape_version\":\"protocol-smoke-contract.synthetic.v1\""));
}

#[test]
fn bound_baseline_preserves_context_and_rejects_mismatches() {
    let mut request = BenchmarkRunRequest::new("protocol-smoke-contract");
    request.duration_ms = 1_000;
    request.samples = 5;
    request.warmups = 1;
    request.hardware_profile = BenchmarkHardwareProfile::Conservative;
    let evidence = evidence_from_request(&request);
    let baseline_target = target(1);

    let baseline = BenchmarkBaseline::from_benchmark_evidence(
        &evidence,
        "baseline-commit",
        "2026-05-06T10:00:00Z",
        baseline_target,
    );
    let json = baseline.to_json();
    assert!(json.contains(r#""source_commit_id":"baseline-commit""#));
    assert!(json.contains(r#""hardware_profile":"conservative""#));
    assert!(json.contains(r#""measurement_mode":"synthetic-diagnostic""#));
    assert!(json.contains(r#""target_catalog_version":9"#));
    assert!(json.contains(r#""target_stats_version":1"#));
    assert!(json.contains(r#""optimizer_boundary":"advisory-only""#));

    let recovered = BenchmarkBaseline::from_json(&json).unwrap();
    assert_eq!(baseline, recovered);

    let analysis =
        RegressionAnalysis::from_baseline_and_evidence(&recovered, &evidence, baseline_target)
            .unwrap();
    assert!(!analysis.is_regressed);

    let stale_stats_target = target(2);
    assert_eq!(
        RegressionAnalysis::from_baseline_and_evidence(&recovered, &evidence, stale_stats_target)
            .unwrap_err(),
        BenchmarkBaselineComparisonError::TargetStatsVersionMismatch
    );

    let legacy_baseline = BenchmarkBaseline::from_evidence(
        evidence.workload_id.clone(),
        evidence.p50_latency_us,
        evidence.p95_latency_us,
        evidence.error_count,
        evidence.sample_count,
        "2026-05-06T10:00:00Z".to_string(),
    );
    assert_eq!(
        RegressionAnalysis::from_baseline_and_evidence(
            &legacy_baseline,
            &evidence,
            baseline_target
        )
        .unwrap_err(),
        BenchmarkBaselineComparisonError::MissingBaselineBinding
    );
}

#[test]
fn baseline_json_parsing_with_missing_fields() {
    let invalid_json = r#"{"workload_id":"test"}"#;
    let result = BenchmarkBaseline::from_json(invalid_json);
    assert!(result.is_err());
}

#[test]
fn regression_analysis_no_regression() {
    let analysis = RegressionAnalysis::new(
        "protocol-smoke-contract".to_string(),
        10_000, // current P50
        10_000, // baseline P50
        50_000, // current P95
        50_000, // baseline P95
        0,      // current errors
        20,     // current samples
        0,      // baseline errors
        20,     // baseline samples
    );

    assert!(!analysis.is_regressed);
    assert_eq!(analysis.primary_reason, RegressionReason::NoRegression);
    assert_eq!(analysis.p50_regression_pct, 0.0);
    assert_eq!(analysis.severity, 0);
}

#[test]
fn regression_analysis_minor_p50_degradation() {
    let analysis = RegressionAnalysis::new(
        "protocol-smoke-contract".to_string(),
        10_250, // current P50 (2.5% degradation)
        10_000, // baseline P50
        50_000, // current P95
        50_000, // baseline P95
        0,
        20,
        0,
        20,
    );

    assert!(!analysis.is_regressed); // At threshold, not over
    assert_eq!(analysis.p50_regression_pct, 2.5);
    assert_eq!(analysis.severity, 0);
}

#[test]
fn regression_analysis_clear_p50_degradation() {
    let analysis = RegressionAnalysis::new(
        "protocol-smoke-contract".to_string(),
        10_300, // current P50 (3% degradation)
        10_000, // baseline P50
        50_000, // current P95
        50_000, // baseline P95
        0,
        20,
        0,
        20,
    );

    assert!(analysis.is_regressed);
    assert_eq!(analysis.primary_reason, RegressionReason::P50Degradation);
    assert_eq!(analysis.severity, 1); // Minor (1-5%)
}

#[test]
fn regression_analysis_moderate_p95_degradation() {
    let analysis = RegressionAnalysis::new(
        "protocol-smoke-contract".to_string(),
        10_000, // current P50
        10_000, // baseline P50
        57_500, // current P95 (15% degradation)
        50_000, // baseline P95
        0,
        20,
        0,
        20,
    );

    assert!(analysis.is_regressed);
    assert_eq!(analysis.primary_reason, RegressionReason::P95Degradation);
    assert_eq!(analysis.severity, 2); // Moderate (5% to <20%)
}

#[test]
fn regression_analysis_severe_latency_degradation() {
    let analysis = RegressionAnalysis::new(
        "protocol-smoke-contract".to_string(),
        10_000, // current P50
        10_000, // baseline P50
        60_500, // current P95 (21% degradation)
        50_000, // baseline P95
        0,
        20,
        0,
        20,
    );

    assert!(analysis.is_regressed);
    assert_eq!(analysis.severity, 3); // Severe (>=20%)
}

#[test]
fn regression_analysis_exact_twenty_percent_degradation_is_severe() {
    let analysis = RegressionAnalysis::new(
        "protocol-smoke-contract".to_string(),
        10_000, // current P50
        10_000, // baseline P50
        60_000, // current P95 (20% degradation)
        50_000, // baseline P95
        0,
        20,
        0,
        20,
    );

    assert!(analysis.is_regressed);
    assert_eq!(analysis.primary_reason, RegressionReason::P95Degradation);
    assert_eq!(analysis.severity, 3); // Severe (>=20%)
}

#[test]
fn regression_analysis_error_rate_increase() {
    let analysis = RegressionAnalysis::new(
        "protocol-smoke-contract".to_string(),
        10_000, // current P50
        10_000, // baseline P50
        50_000, // current P95
        50_000, // baseline P95
        5,      // current errors
        100,    // current samples
        0,      // baseline errors
        100,    // baseline samples
    );

    assert!(analysis.is_regressed);
    assert_eq!(analysis.primary_reason, RegressionReason::ErrorRateIncrease);
    assert_eq!(analysis.error_rate_regression_pct, 100.0);
}

#[test]
fn regression_analysis_error_rate_uses_sample_counts() {
    let same_error_count_higher_rate = RegressionAnalysis::new(
        "protocol-smoke-contract".to_string(),
        10_000,
        10_000,
        50_000,
        50_000,
        1,
        10,
        1,
        100,
    );

    assert!(same_error_count_higher_rate.is_regressed);
    assert_eq!(
        same_error_count_higher_rate.primary_reason,
        RegressionReason::ErrorRateIncrease
    );
    assert_eq!(same_error_count_higher_rate.current_error_rate_ppm, 100_000);
    assert_eq!(same_error_count_higher_rate.baseline_error_rate_ppm, 10_000);

    let higher_count_lower_rate = RegressionAnalysis::new(
        "protocol-smoke-contract".to_string(),
        10_000,
        10_000,
        50_000,
        50_000,
        5,
        1_000,
        1,
        100,
    );

    assert!(!higher_count_lower_rate.is_regressed);
    assert_eq!(
        higher_count_lower_rate.primary_reason,
        RegressionReason::NoRegression
    );
    assert!(higher_count_lower_rate.error_rate_regression_pct < 0.0);
}

#[test]
fn regression_analysis_multiple_metrics_degradation() {
    let analysis = RegressionAnalysis::new(
        "protocol-smoke-contract".to_string(),
        10_500, // P50 +5%
        10_000, // baseline P50
        60_000, // P95 +20%
        50_000, // baseline P95
        2,      // errors
        20,     // current samples
        0,      // baseline errors
        20,     // baseline samples
    );

    assert!(analysis.is_regressed);
    assert_eq!(analysis.primary_reason, RegressionReason::MultipleMetrics);
    assert_eq!(analysis.severity, 3); // Max severity due to severe P95 degradation
}

#[test]
fn regression_analysis_improvement() {
    let analysis = RegressionAnalysis::new(
        "protocol-smoke-contract".to_string(),
        9_500,  // current P50 (5% improvement)
        10_000, // baseline P50
        45_000, // current P95 (10% improvement)
        50_000, // baseline P95
        0,
        20,
        0,
        20,
    );

    assert!(!analysis.is_regressed);
    assert_eq!(analysis.p50_regression_pct, -5.0);
    assert_eq!(analysis.severity, 0);
}

#[test]
fn regression_analysis_json_serialization() {
    let analysis = RegressionAnalysis::new(
        "btree-lookup-smoke".to_string(),
        26,  // P50
        25,  // baseline P50
        510, // P95
        500, // baseline P95
        0,
        20,
        0,
        20,
    );

    let json = analysis.to_json();
    assert!(json.contains("\"workload_id\":\"btree-lookup-smoke\""));
    assert!(json.contains("\"current_p50_us\":26"));
    assert!(json.contains("\"p50_regression_pct\":4.00")); // 4% over threshold
    assert!(json.contains("\"current_sample_count\":20"));
    assert!(json.contains("\"baseline_sample_count\":20"));
    assert!(json.contains("\"is_regressed\":true"));
}

#[test]
fn regression_analysis_is_diagnostic_not_production_truth() {
    let analysis = RegressionAnalysis::new(
        "protocol-smoke-contract".to_string(),
        10_300,
        10_000,
        51_000,
        50_000,
        0,
        20,
        0,
        20,
    );

    assert!(analysis.is_regressed);
    assert!(!analysis.is_production_truth());
    assert!(!analysis.is_authoritative());
    assert!(!analysis.can_select_plan_alone());
    assert_eq!(analysis.optimizer_consumption_role(), "advisory-only");

    let json = analysis.to_json();
    assert!(json.contains("\"production_truth\":false"));
    assert!(json.contains("\"authoritative\":false"));
    assert!(json.contains("\"can_select_plan_alone\":false"));
    assert!(json.contains("\"optimizer_boundary\":\"advisory-only\""));
}

#[test]
fn regression_percentage_computation() {
    assert_eq!(percent_change(100, 100), 0.0); // No change
    assert_eq!(percent_change(150, 100), 50.0); // +50%
    assert_eq!(percent_change(50, 100), -50.0); // -50%
    assert_eq!(percent_change(1, 0), 100.0); // Division by zero handled
    assert_eq!(percent_change(0, 0), 0.0); // Both zero
    assert_eq!(percent_change(0, 1), -100.0); // Current zero
    assert!(percent_change(1, 0).is_finite());
    assert!(percent_change(0, 0).is_finite());
    assert!(percent_change(0, 1).is_finite());
}

#[test]
fn regression_analysis_zero_baselines_produce_finite_percentages() {
    let analysis = RegressionAnalysis::new(
        "protocol-smoke-contract".to_string(),
        0,
        0,
        1,
        0,
        0,
        0,
        0,
        0,
    );

    assert!(analysis.p50_regression_pct.is_finite());
    assert!(analysis.p95_regression_pct.is_finite());
    assert!(analysis.error_rate_regression_pct.is_finite());
    assert_eq!(analysis.p50_regression_pct, 0.0);
    assert_eq!(analysis.p95_regression_pct, 100.0);
}
