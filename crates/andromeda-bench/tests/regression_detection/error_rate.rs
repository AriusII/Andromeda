use andromeda_bench::{BenchmarkBaseline, RegressionAnalysis, RegressionReason};

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
        100,
        baseline.error_count,
        baseline.sample_count,
    );

    assert!(analysis.is_regressed);
    assert_eq!(analysis.error_rate_regression_pct, 100.0);
    assert_eq!(analysis.primary_reason, RegressionReason::ErrorRateIncrease);
    assert_eq!(analysis.severity, 3);
    assert_eq!(analysis.primary_reason.as_str(), "error-rate-increase");
}

#[test]
fn error_rate_regression_uses_rate_not_raw_counts() {
    let same_count_higher_rate = RegressionAnalysis::new(
        "inventory-reserve-stock".to_string(),
        15_000,
        15_000,
        50_000,
        50_000,
        1,
        10,
        1,
        100,
    );

    assert!(same_count_higher_rate.is_regressed);
    assert_eq!(
        same_count_higher_rate.primary_reason,
        RegressionReason::ErrorRateIncrease
    );

    let higher_count_lower_rate = RegressionAnalysis::new(
        "inventory-reserve-stock".to_string(),
        15_000,
        15_000,
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
}
