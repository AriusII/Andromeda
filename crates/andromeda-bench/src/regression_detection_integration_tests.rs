use crate::{BenchmarkBaseline, RegressionAnalysis, RegressionReason};

#[test]
fn end_to_end_regression_detection_workflow() {
    let baseline = BenchmarkBaseline::from_evidence(
        "protocol-smoke-contract".to_string(),
        10_000,
        50_000,
        0,
        20,
        "2026-01-15T14:30:45Z".to_string(),
    );

    let baseline_json = baseline.to_json();
    assert!(baseline_json.contains("protocol-smoke-contract"));

    let recovered_baseline = BenchmarkBaseline::from_json(&baseline_json).unwrap();
    assert_eq!(baseline, recovered_baseline);

    let current_p50 = 10_300;
    let current_p95 = 51_000;
    let current_errors = 0;

    let analysis = RegressionAnalysis::new(
        "protocol-smoke-contract".to_string(),
        current_p50,
        baseline.p50_latency_us,
        current_p95,
        baseline.p95_latency_us,
        current_errors,
        baseline.error_count,
    );

    assert!(analysis.is_regressed);
    assert_eq!(analysis.p50_regression_pct, 3.0);
    assert_eq!(analysis.primary_reason, RegressionReason::P50Degradation);
    assert_eq!(analysis.severity, 1);
    assert_eq!(analysis.primary_reason.as_str(), "p50-latency-degradation");

    let analysis_json = analysis.to_json();
    assert!(analysis_json.contains("is_regressed"));
    assert!(analysis_json.contains("true"));
}

#[test]
fn baseline_update_workflow() {
    let baseline_v1 = BenchmarkBaseline::from_evidence(
        "btree-lookup-smoke".to_string(),
        25,
        500,
        0,
        20,
        "2026-01-15T00:00:00Z".to_string(),
    );

    let analysis_v1 = RegressionAnalysis::new(
        "btree-lookup-smoke".to_string(),
        30,
        baseline_v1.p50_latency_us,
        530,
        baseline_v1.p95_latency_us,
        0,
        baseline_v1.error_count,
    );

    assert!(analysis_v1.is_regressed);
    assert_eq!(analysis_v1.severity, 3);

    let baseline_v2 = BenchmarkBaseline::from_evidence(
        "btree-lookup-smoke".to_string(),
        26,
        495,
        0,
        20,
        "2026-01-16T00:00:00Z".to_string(),
    );

    let analysis_v2 = RegressionAnalysis::new(
        "btree-lookup-smoke".to_string(),
        26,
        baseline_v2.p50_latency_us,
        495,
        baseline_v2.p95_latency_us,
        0,
        baseline_v2.error_count,
    );

    assert!(!analysis_v2.is_regressed);
    assert_eq!(analysis_v2.severity, 0);
}

#[test]
fn multi_workload_regression_tracking() {
    let workloads = [
        "btree-lookup-smoke",
        "btree-range-scan-smoke",
        "vertical-v0-smoke",
        "protocol-smoke-contract",
        "wal-append-smoke",
    ];

    let mut baselines = Vec::new();
    let mut analyses = Vec::new();

    for workload_id in &workloads {
        let baseline = BenchmarkBaseline::from_evidence(
            workload_id.to_string(),
            1000,
            5000,
            0,
            20,
            "2026-01-15T00:00:00Z".to_string(),
        );
        baselines.push(baseline.clone());

        let current_p50 = 1010;
        let current_p95 = 5040;

        let analysis = RegressionAnalysis::new(
            workload_id.to_string(),
            current_p50,
            baseline.p50_latency_us,
            current_p95,
            baseline.p95_latency_us,
            0,
            baseline.error_count,
        );
        analyses.push(analysis);
    }

    for analysis in &analyses {
        assert!(
            !analysis.is_regressed,
            "workload {} regressed",
            analysis.workload_id
        );
    }

    let bad_analysis = RegressionAnalysis::new(
        "protocol-smoke-contract".to_string(),
        10_300,
        10_000,
        50_000,
        50_000,
        0,
        0,
    );

    assert!(bad_analysis.is_regressed);
    assert_eq!(bad_analysis.severity, 1);

    assert_eq!(
        bad_analysis.primary_reason,
        RegressionReason::P50Degradation
    );
}

#[test]
fn error_rate_regression_critical() {
    let baseline = BenchmarkBaseline::from_evidence(
        "inventory-reserve-stock".to_string(),
        15_000,
        50_000,
        0,
        100,
        "2026-01-15T00:00:00Z".to_string(),
    );

    let analysis = RegressionAnalysis::new(
        "inventory-reserve-stock".to_string(),
        15_100,
        baseline.p50_latency_us,
        50_200,
        baseline.p95_latency_us,
        5,
        baseline.error_count,
    );

    assert!(analysis.is_regressed);
    assert_eq!(analysis.error_rate_regression_pct, 100.0);
    assert_eq!(analysis.primary_reason, RegressionReason::ErrorRateIncrease);
    assert_eq!(analysis.severity, 3);

    assert_eq!(analysis.primary_reason.as_str(), "error-rate-increase");
}

#[test]
fn regression_analysis_for_observability() {
    let analysis = RegressionAnalysis::new(
        "wal-append-smoke".to_string(),
        20_600,
        20_000,
        75_000,
        75_000,
        0,
        0,
    );

    let json = analysis.to_json();

    assert!(json.contains("\"workload_id\":\"wal-append-smoke\""));
    assert!(json.contains("\"p50_regression_pct\":3.00"));
    assert!(json.contains("\"is_regressed\":true"));
    assert!(json.contains("\"severity\":1"));
    assert!(json.contains("\"primary_reason\":\"p50-latency-degradation\""));
}

#[test]
fn false_positive_prevention() {
    let baseline = BenchmarkBaseline::from_evidence(
        "catalog-resolve-procedure".to_string(),
        800,
        4_200,
        0,
        20,
        "2026-01-15T00:00:00Z".to_string(),
    );

    let analysis = RegressionAnalysis::new(
        "catalog-resolve-procedure".to_string(),
        787,
        baseline.p50_latency_us,
        4_080,
        baseline.p95_latency_us,
        0,
        baseline.error_count,
    );

    assert!(!analysis.is_regressed);
    assert_eq!(analysis.severity, 0);
    assert_eq!(analysis.primary_reason, RegressionReason::NoRegression);

    assert!(analysis.p50_regression_pct < 0.0);
}

#[test]
fn baseline_json_validation() {
    let valid_json = r#"{
        "workload_id": "test",
        "p50_latency_us": 100,
        "p95_latency_us": 500,
        "error_count": 0,
        "sample_count": 20,
        "established_at": "2026-01-15T00:00:00Z"
    }"#;

    let baseline = BenchmarkBaseline::from_json(valid_json).unwrap();
    assert_eq!(baseline.workload_id, "test");
    assert_eq!(baseline.p50_latency_us, 100);

    let invalid_json = r#"{"workload_id": "test"}"#;
    let result = BenchmarkBaseline::from_json(invalid_json);
    assert!(result.is_err());
}

#[test]
fn reproducibility_across_ci_runs() {
    let run1 = RegressionAnalysis::new("btree-lookup-smoke".to_string(), 30, 25, 550, 500, 1, 0);

    let run2 = RegressionAnalysis::new("btree-lookup-smoke".to_string(), 30, 25, 550, 500, 1, 0);

    // Must be identical
    assert_eq!(run1, run2);
    assert_eq!(run1.is_regressed, run2.is_regressed);
    assert_eq!(run1.severity, run2.severity);
    assert_eq!(run1.primary_reason, run2.primary_reason);
    assert_eq!(run1.to_json(), run2.to_json());
}
