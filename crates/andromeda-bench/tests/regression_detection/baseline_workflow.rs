use andromeda_regression::{BenchmarkBaseline, RegressionAnalysis, RegressionReason};

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
        20,
        baseline.error_count,
        baseline.sample_count,
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
        20,
        baseline_v1.error_count,
        baseline_v1.sample_count,
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
        20,
        baseline_v2.error_count,
        baseline_v2.sample_count,
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

        let current_p50 = 1010;
        let current_p95 = 5040;

        let analysis = RegressionAnalysis::new(
            workload_id.to_string(),
            current_p50,
            baseline.p50_latency_us,
            current_p95,
            baseline.p95_latency_us,
            0,
            20,
            baseline.error_count,
            baseline.sample_count,
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
        20,
        0,
        20,
    );

    assert!(bad_analysis.is_regressed);
    assert_eq!(bad_analysis.severity, 1);
    assert_eq!(
        bad_analysis.primary_reason,
        RegressionReason::P50Degradation
    );
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
        20,
        baseline.error_count,
        baseline.sample_count,
    );

    assert!(!analysis.is_regressed);
    assert_eq!(analysis.severity, 0);
    assert_eq!(analysis.primary_reason, RegressionReason::NoRegression);
    assert!(analysis.p50_regression_pct < 0.0);
}

#[test]
fn reproducibility_across_ci_runs() {
    let run1 = RegressionAnalysis::new(
        "btree-lookup-smoke".to_string(),
        30,
        25,
        550,
        500,
        1,
        20,
        0,
        20,
    );

    let run2 = RegressionAnalysis::new(
        "btree-lookup-smoke".to_string(),
        30,
        25,
        550,
        500,
        1,
        20,
        0,
        20,
    );

    assert_eq!(run1, run2);
    assert_eq!(run1.is_regressed, run2.is_regressed);
    assert_eq!(run1.severity, run2.severity);
    assert_eq!(run1.primary_reason, run2.primary_reason);
    assert_eq!(run1.to_json(), run2.to_json());
}
