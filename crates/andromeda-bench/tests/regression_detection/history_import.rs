use andromeda_bench::{
    BenchmarkHistoryRecord, BenchmarkHistoryStore, HistoryQuery, RegressionAnalysis,
    RegressionReason,
};

#[test]
fn history_import_loads_baseline_for_regression_analysis() {
    let baseline_record = BenchmarkHistoryRecord::new(
        "wal-append-file-smoke".to_string(),
        "baseline-commit".to_string(),
        "2026-01-15T00:00:00Z".to_string(),
        1_000,
        4_000,
        0,
        20,
    );
    let current_record = BenchmarkHistoryRecord::new(
        "wal-append-file-smoke".to_string(),
        "current-commit".to_string(),
        "2026-01-16T00:00:00Z".to_string(),
        1_025,
        4_100,
        0,
        20,
    );
    let json_lines = format!(
        "{}\n{}\n",
        baseline_record.to_json_line(),
        current_record.to_json_line()
    );

    let mut store = BenchmarkHistoryStore::from_file("target/bench-history.jsonl").unwrap();
    assert_eq!(store.import_json_lines(&json_lines).unwrap(), 2);

    let history = store
        .query("wal-append-file-smoke", HistoryQuery::LastNCommits(2))
        .unwrap();
    let baseline = &history.records[0];
    let current = &history.records[1];
    let analysis = RegressionAnalysis::new(
        current.workload_id.clone(),
        current.p50_latency_us,
        baseline.p50_latency_us,
        current.p95_latency_us,
        baseline.p95_latency_us,
        current.error_count,
        current.sample_count,
        baseline.error_count,
        baseline.sample_count,
    );

    assert!(!analysis.is_regressed);
    assert_eq!(analysis.p50_regression_pct, 2.5);
    assert_eq!(analysis.primary_reason, RegressionReason::NoRegression);
}
