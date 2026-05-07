use andromeda_bench::{
    BenchmarkBaseline, BenchmarkBaselineComparisonError, BenchmarkRunRequest, RegressionAnalysis,
    run_bounded_benchmark,
};

use crate::common::scenario_target;

#[test]
fn bound_baseline_rejects_incomparable_target_context() {
    let mut request = BenchmarkRunRequest::new("protocol-smoke-contract");
    request.duration_ms = 1_000;
    request.samples = 5;
    request.warmups = 1;
    let evidence = run_bounded_benchmark(&request).unwrap();
    let target = scenario_target(1);
    let baseline = BenchmarkBaseline::from_benchmark_evidence(
        &evidence,
        "baseline-commit",
        "2026-05-06T10:00:00Z",
        target,
    );

    let json = baseline.to_json();
    assert!(json.contains(r#""target_contract_hash":"#));
    assert!(json.contains(r#""duration_budget_ms":1000"#));
    assert!(json.contains(r#""sample_budget":5"#));
    assert!(json.contains(r#""optimizer_boundary":"advisory-only""#));

    let recovered = BenchmarkBaseline::from_json(&json).unwrap();
    assert!(RegressionAnalysis::from_baseline_and_evidence(&recovered, &evidence, target).is_ok());

    assert_eq!(
        RegressionAnalysis::from_baseline_and_evidence(&recovered, &evidence, scenario_target(2))
            .unwrap_err(),
        BenchmarkBaselineComparisonError::TargetStatsVersionMismatch
    );
}
