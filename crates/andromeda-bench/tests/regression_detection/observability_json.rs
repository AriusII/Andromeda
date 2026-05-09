use andromeda_regression::{BenchmarkBaseline, RegressionAnalysis};

#[test]
fn regression_analysis_for_observability() {
    let analysis = RegressionAnalysis::new(
        "wal-append-smoke".to_string(),
        20_600,
        20_000,
        75_000,
        75_000,
        0,
        20,
        0,
        20,
    );

    let json = analysis.to_json();

    assert!(json.contains("\"workload_id\":\"wal-append-smoke\""));
    assert!(json.contains("\"p50_regression_pct\":3.00"));
    assert!(json.contains("\"current_sample_count\":20"));
    assert!(json.contains("\"baseline_sample_count\":20"));
    assert!(json.contains("\"current_error_rate_ppm\":0"));
    assert!(json.contains("\"baseline_error_rate_ppm\":0"));
    assert!(json.contains("\"is_regressed\":true"));
    assert!(json.contains("\"severity\":1"));
    assert!(json.contains("\"primary_reason\":\"p50-latency-degradation\""));
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
