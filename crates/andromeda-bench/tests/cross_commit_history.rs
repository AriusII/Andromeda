#![forbid(unsafe_code)]

//! Cross-commit benchmark history integration tests.

use andromeda_bench::{BenchmarkHistoryRecord, BenchmarkHistoryStore, HistoryQuery, TimeRange};

#[test]
fn detects_regression_onset_across_commits() {
    let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

    let commits = [
        ("abc001", "2026-01-15T10:00:00Z", 100, 500, 0),
        ("abc002", "2026-01-15T10:10:00Z", 101, 501, 0),
        ("abc003", "2026-01-15T10:20:00Z", 100, 502, 0),
        ("abc004", "2026-01-15T10:30:00Z", 102, 500, 0),
        ("abc005", "2026-01-15T10:40:00Z", 125, 525, 1),
        ("abc006", "2026-01-15T10:50:00Z", 130, 530, 1),
        ("abc007", "2026-01-15T11:00:00Z", 135, 540, 2),
        ("abc008", "2026-01-15T11:10:00Z", 140, 550, 2),
        ("abc009", "2026-01-15T11:20:00Z", 145, 560, 3),
        ("abc010", "2026-01-15T11:30:00Z", 150, 570, 3),
    ];

    for (commit_id, timestamp, p50, p95, errors) in commits {
        let record = BenchmarkHistoryRecord::new(
            "btree-lookup-smoke".to_string(),
            commit_id.to_string(),
            timestamp.to_string(),
            p50,
            p95,
            errors,
            20,
        );
        store.append(record).unwrap();
    }

    let result = store
        .query("btree-lookup-smoke", HistoryQuery::LastNCommits(10))
        .unwrap();

    assert_eq!(result.records.len(), 10);

    let trend = result.trend_p50().unwrap();
    assert!(trend > 40.0);

    let regression_point = result.find_regression_point(5.0).unwrap();
    assert_eq!(regression_point.0, "abc005");
    assert!(regression_point.1 > 20.0);
}

#[test]
fn tracks_performance_improvement_across_commits() {
    let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

    let commits = [
        ("v1", "2026-01-15T10:00:00Z", 200, 1000, 0),
        ("v2", "2026-01-15T10:10:00Z", 180, 900, 0),
        ("v3", "2026-01-15T10:20:00Z", 150, 750, 0),
        ("v4", "2026-01-15T10:30:00Z", 120, 600, 0),
        ("v5", "2026-01-15T10:40:00Z", 100, 500, 0),
    ];

    for (commit_id, timestamp, p50, p95, errors) in commits {
        let record = BenchmarkHistoryRecord::new(
            "wal-append-smoke".to_string(),
            commit_id.to_string(),
            timestamp.to_string(),
            p50,
            p95,
            errors,
            20,
        );
        store.append(record).unwrap();
    }

    let result = store
        .query("wal-append-smoke", HistoryQuery::AllForWorkload)
        .unwrap();

    let trend = result.trend_p50().unwrap();
    assert!(trend < -40.0);

    assert!(result.is_improving(1.0));
}

#[test]
fn tracks_error_rate_regression_across_commits() {
    let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

    let commits = [
        ("c1", "2026-01-15T10:00:00Z", 100, 500, 0),
        ("c2", "2026-01-15T10:10:00Z", 101, 501, 0),
        ("c3", "2026-01-15T10:20:00Z", 102, 502, 1),
        ("c4", "2026-01-15T10:30:00Z", 105, 510, 2),
        ("c5", "2026-01-15T10:40:00Z", 110, 520, 5),
    ];

    for (commit_id, timestamp, p50, p95, errors) in commits {
        let record = BenchmarkHistoryRecord::new(
            "inventory-reserve".to_string(),
            commit_id.to_string(),
            timestamp.to_string(),
            p50,
            p95,
            errors,
            100,
        );
        store.append(record).unwrap();
    }

    let result = store
        .query("inventory-reserve", HistoryQuery::AllForWorkload)
        .unwrap();

    assert_eq!(result.records.len(), 5);

    let last = result.records.last().unwrap();
    assert_eq!(last.error_count, 5);

    let is_regressed = last.is_regressed_vs_baseline(100, 500, 2.5);
    assert!(is_regressed);
}

#[test]
fn stores_history_for_multiple_workloads() {
    let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

    let workloads = [
        "btree-lookup-smoke",
        "btree-range-scan-smoke",
        "wal-append-smoke",
        "protocol-smoke-contract",
    ];

    for (idx, workload) in workloads.iter().enumerate() {
        for commit_idx in 0..5 {
            let record = BenchmarkHistoryRecord::new(
                workload.to_string(),
                format!("c{}", commit_idx),
                format!("2026-01-15T10:{:02}:00Z", commit_idx * 10),
                100 + (idx as u64 * 100) + (commit_idx as u64 * 5),
                500 + (idx as u64 * 100) + (commit_idx as u64 * 10),
                0,
                20,
            );
            store.append(record).unwrap();
        }
    }

    assert_eq!(store.total_records(), 20);
    assert_eq!(store.workload_ids().len(), 4);

    for workload in &workloads {
        let result = store.query(workload, HistoryQuery::AllForWorkload).unwrap();
        assert_eq!(result.records.len(), 5);
    }
}

#[test]
fn serializes_history_to_json_lines_and_imports_it() {
    let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

    let records = vec![
        BenchmarkHistoryRecord::new(
            "test".to_string(),
            "commit1".to_string(),
            "2026-01-15T12:00:00Z".to_string(),
            100,
            500,
            0,
            20,
        )
        .with_context(Some("main".to_string()), Some(123)),
        BenchmarkHistoryRecord::new(
            "test".to_string(),
            "commit2".to_string(),
            "2026-01-15T13:00:00Z".to_string(),
            110,
            510,
            1,
            20,
        )
        .with_context(Some("feature/perf".to_string()), Some(124)),
    ];

    for record in records {
        store.append(record).unwrap();
    }

    let json_output = store.save().unwrap();

    let mut new_store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());
    let imported = new_store.import_json_lines(&json_output).unwrap();

    assert_eq!(imported, 2);
    assert_eq!(new_store.total_records(), 2);

    let result = new_store
        .query("test", HistoryQuery::AllForWorkload)
        .unwrap();

    let first = &result.records[0];
    assert_eq!(first.branch, Some("main".to_string()));
    assert_eq!(first.pr_number, Some(123));

    let second = &result.records[1];
    assert_eq!(second.error_count, 1);
}

#[test]
fn filters_history_by_time_range() {
    let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

    let records = [
        ("c1", "2026-01-15T00:00:00Z", 100, 500),
        ("c2", "2026-01-15T08:00:00Z", 105, 510),
        ("c3", "2026-01-15T12:00:00Z", 110, 520),
        ("c4", "2026-01-15T16:00:00Z", 115, 530),
        ("c5", "2026-01-15T23:59:59Z", 120, 540),
    ];

    for (commit, timestamp, p50, p95) in records {
        let record = BenchmarkHistoryRecord::new(
            "test".to_string(),
            commit.to_string(),
            timestamp.to_string(),
            p50,
            p95,
            0,
            20,
        );
        store.append(record).unwrap();
    }

    let range = TimeRange::new(
        "2026-01-15T08:00:00Z".to_string(),
        "2026-01-15T16:00:00Z".to_string(),
    );

    let result = store.query("test", HistoryQuery::TimeRange(range)).unwrap();

    assert_eq!(result.records.len(), 3);
    assert_eq!(result.records[0].commit_id, "c2");
    assert_eq!(result.records[2].commit_id, "c4");
}

#[test]
fn filters_history_by_commit_id() {
    let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

    for i in 1..=5 {
        let record = BenchmarkHistoryRecord::new(
            "test".to_string(),
            format!("abc{:03}", i),
            format!("2026-01-15T{:02}:00:00Z", 10 + i),
            100 + (i as u64 * 10),
            500 + (i as u64 * 10),
            0,
            20,
        );
        store.append(record).unwrap();
    }

    let result = store
        .query("test", HistoryQuery::CommitId("abc003".to_string()))
        .unwrap();

    assert_eq!(result.records.len(), 1);
    assert_eq!(result.records[0].p50_latency_us, 130);
    assert_eq!(result.records[0].p95_latency_us, 530);
}

#[test]
fn stable_performance_does_not_create_regression_point() {
    let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

    let commits = [
        ("c1", 100, 500),
        ("c2", 101, 502),
        ("c3", 100, 500),
        ("c4", 102, 501),
        ("c5", 100, 500),
    ];

    for (idx, (commit, p50, p95)) in commits.iter().enumerate() {
        let record = BenchmarkHistoryRecord::new(
            "test".to_string(),
            commit.to_string(),
            format!("2026-01-15T10:{:02}:00Z", idx * 10),
            *p50,
            *p95,
            0,
            20,
        );
        store.append(record).unwrap();
    }

    let result = store.query("test", HistoryQuery::AllForWorkload).unwrap();

    let trend = result.trend_p50().unwrap();
    assert!(trend.abs() < 2.0);

    let regression = result.find_regression_point(5.0);
    assert!(regression.is_none());
}

#[test]
fn clearing_store_removes_history_records() {
    let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

    for i in 1..=5 {
        let record = BenchmarkHistoryRecord::new(
            "test".to_string(),
            format!("c{}", i),
            format!("2026-01-15T{:02}:00:00Z", 10 + i),
            100,
            500,
            0,
            20,
        );
        store.append(record).unwrap();
    }

    assert_eq!(store.total_records(), 5);
    store.clear();
    assert_eq!(store.total_records(), 0);

    let result = store.query("test", HistoryQuery::AllForWorkload);
    assert!(result.is_err());
}

#[test]
fn regression_gate_workflow_updates_history_and_finds_regression() {
    let mut history_store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

    let historical_json_lines = r#"{"workload_id":"protocol-smoke-contract","commit_id":"abc001","timestamp":"2026-01-15T10:00:00Z","p50_latency_us":10000,"p95_latency_us":50000,"error_count":0,"sample_count":20,"branch":"main","pr_number":null}
{"workload_id":"protocol-smoke-contract","commit_id":"abc002","timestamp":"2026-01-15T10:10:00Z","p50_latency_us":10100,"p95_latency_us":50200,"error_count":0,"sample_count":20,"branch":"main","pr_number":null}
{"workload_id":"protocol-smoke-contract","commit_id":"abc003","timestamp":"2026-01-15T10:20:00Z","p50_latency_us":10200,"p95_latency_us":50400,"error_count":0,"sample_count":20,"branch":"main","pr_number":null}"#;

    history_store
        .import_json_lines(historical_json_lines)
        .unwrap();

    let current_record = BenchmarkHistoryRecord::new(
        "protocol-smoke-contract".to_string(),
        "abc004".to_string(),
        "2026-01-15T10:30:00Z".to_string(),
        10_500,
        51_000,
        0,
        20,
    )
    .with_context(Some("main".to_string()), Some(99));

    history_store.append(current_record.clone()).unwrap();

    let result = history_store
        .query("protocol-smoke-contract", HistoryQuery::LastNCommits(4))
        .unwrap();

    assert_eq!(result.records.len(), 4);

    let baseline_p50 = 10_000;
    let baseline_p95 = 50_000;
    let is_regressed = current_record.is_regressed_vs_baseline(baseline_p50, baseline_p95, 2.5);

    assert!(is_regressed);

    let regression_point = result.find_regression_point(2.0);
    assert!(regression_point.is_some());

    let updated_json = history_store.save().unwrap();
    assert!(updated_json.contains("abc004"));
}

#[test]
fn empty_store_reports_no_history() {
    let store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

    assert_eq!(store.total_records(), 0);
    assert!(store.workload_ids().is_empty());

    let result = store.query("nonexistent", HistoryQuery::AllForWorkload);
    assert!(result.is_err());
}

#[test]
fn preserves_branch_and_pr_metadata() {
    let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

    let record = BenchmarkHistoryRecord::new(
        "test".to_string(),
        "commit-abc".to_string(),
        "2026-01-15T12:00:00Z".to_string(),
        100,
        500,
        0,
        20,
    )
    .with_context(Some("feature/performance".to_string()), Some(456));

    store.append(record.clone()).unwrap();

    let result = store
        .query("test", HistoryQuery::CommitId("commit-abc".to_string()))
        .unwrap();

    let retrieved = &result.records[0];
    assert_eq!(retrieved.branch, Some("feature/performance".to_string()));
    assert_eq!(retrieved.pr_number, Some(456));
}
