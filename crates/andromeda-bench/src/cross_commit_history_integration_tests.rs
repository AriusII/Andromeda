#![forbid(unsafe_code)]

//! Cross-Commit Benchmark History Integration Tests — Wave 13
//!
//! This test suite demonstrates the complete workflow for:
//! 1. Recording benchmark evidence at each commit
//! 2. Querying historical trends
//! 3. Detecting performance regressions
//! 4. Identifying when regressions started
//!
//! All tests are deterministic and use hard-coded timestamps to ensure
//! reproducibility across CI runs.

#[cfg(test)]
mod cross_commit_history_integration_tests {
    use crate::{BenchmarkHistoryRecord, BenchmarkHistoryStore, HistoryQuery, TimeRange};

    /// Simulates a series of commits with gradually degrading performance.
    /// This demonstrates how Wave 14 CI will detect when a regression occurred.
    #[test]
    fn test_detect_regression_onset() {
        let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

        // Simulate 10 commits, all stable until commit 5
        let commits = vec![
            ("abc001", "2026-01-15T10:00:00Z", 100, 500, 0),
            ("abc002", "2026-01-15T10:10:00Z", 101, 501, 0),
            ("abc003", "2026-01-15T10:20:00Z", 100, 502, 0),
            ("abc004", "2026-01-15T10:30:00Z", 102, 500, 0),
            ("abc005", "2026-01-15T10:40:00Z", 125, 525, 1), // Regression starts!
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

        // Query last 10 commits
        let result = store
            .query("btree-lookup-smoke", HistoryQuery::LastNCommits(10))
            .unwrap();

        assert_eq!(result.records.len(), 10);

        // Detect overall trend (10% degradation over period)
        let trend = result.trend_p50().unwrap();
        assert!(trend > 40.0); // Roughly 50% degradation from 100 to 150

        // Find where regression started (should be around commit 5)
        let regression_point = result.find_regression_point(5.0).unwrap();
        assert_eq!(regression_point.0, "abc005");
        assert!(regression_point.1 > 20.0); // ~25% jump vs commit 4
    }

    /// Demonstrates that history accurately tracks performance improvements.
    #[test]
    fn test_track_performance_improvement() {
        let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

        // Simulate performance getting progressively better
        let commits = vec![
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

        // Trend should show improvement (negative percentage)
        let trend = result.trend_p50().unwrap();
        assert!(trend < -40.0); // 50% improvement

        // Verify we're improving (each step better than previous)
        assert!(result.is_improving(1.0)); // Threshold 1%
    }

    /// Demonstrates error rate detection across commits.
    #[test]
    fn test_error_rate_regression_tracking() {
        let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

        // Simulate error rate creeping up
        let commits = vec![
            ("c1", "2026-01-15T10:00:00Z", 100, 500, 0),
            ("c2", "2026-01-15T10:10:00Z", 101, 501, 0),
            ("c3", "2026-01-15T10:20:00Z", 102, 502, 1), // Errors appear
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

        // Last record has 5 errors (regression vs baseline of 0)
        let last = result.records.last().unwrap();
        assert_eq!(last.error_count, 5);

        // Regression detection vs baseline
        let is_regressed = last.is_regressed_vs_baseline(100, 500, 2.5);
        assert!(is_regressed);
    }

    /// Demonstrates multi-workload history tracking.
    /// In Wave 14, CI will maintain separate baselines for each workload.
    #[test]
    fn test_multi_workload_history() {
        let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

        let workloads = vec![
            "btree-lookup-smoke",
            "btree-range-scan-smoke",
            "wal-append-smoke",
            "protocol-smoke-contract",
        ];

        // Add history for each workload
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

        // Verify store maintains separate history per workload
        assert_eq!(store.total_records(), 20); // 4 workloads * 5 commits
        assert_eq!(store.workload_ids().len(), 4);

        for workload in &workloads {
            let result = store.query(workload, HistoryQuery::AllForWorkload).unwrap();
            assert_eq!(result.records.len(), 5);
        }
    }

    /// Demonstrates JSON serialization round-trip for artifact storage.
    /// Wave 14 will use this to save/load from GitHub Actions artifact store.
    #[test]
    fn test_json_serialization_round_trip() {
        let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

        // Create diverse records
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

        // Serialize to JSON Lines
        let json_output = store.save().unwrap();

        // Create new store and deserialize
        let mut new_store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());
        let imported = new_store.import_json_lines(&json_output).unwrap();

        assert_eq!(imported, 2);
        assert_eq!(new_store.total_records(), 2);

        // Verify all fields round-trip correctly
        let result = new_store
            .query("test", HistoryQuery::AllForWorkload)
            .unwrap();

        let first = &result.records[0];
        assert_eq!(first.branch, Some("main".to_string()));
        assert_eq!(first.pr_number, Some(123));

        let second = &result.records[1];
        assert_eq!(second.error_count, 1);
    }

    /// Demonstrates time-based queries for historical analysis.
    #[test]
    fn test_time_range_queries() {
        let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

        // Create records over a 24-hour period
        let records = vec![
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

        // Query business hours only
        let range = TimeRange::new(
            "2026-01-15T08:00:00Z".to_string(),
            "2026-01-15T16:00:00Z".to_string(),
        );

        let result = store.query("test", HistoryQuery::TimeRange(range)).unwrap();

        assert_eq!(result.records.len(), 3);
        assert_eq!(result.records[0].commit_id, "c2");
        assert_eq!(result.records[2].commit_id, "c4");
    }

    /// Demonstrates commit-specific queries.
    #[test]
    fn test_query_specific_commit() {
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

    /// Demonstrates that history correctly identifies stable performance.
    #[test]
    fn test_stable_performance_no_regression() {
        let mut store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

        // Simulate perfectly stable performance (within natural variance)
        let commits = vec![
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

        // Overall trend should be minimal (< 2%)
        let trend = result.trend_p50().unwrap();
        assert!(trend.abs() < 2.0);

        // Should find no significant regression point
        let regression = result.find_regression_point(5.0);
        assert!(regression.is_none());
    }

    /// Demonstrates clearing history (useful for testing and reset scenarios).
    #[test]
    fn test_store_clear() {
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

        // Query should fail after clear
        let result = store.query("test", HistoryQuery::AllForWorkload);
        assert!(result.is_err());
    }

    /// Simulates a complete Wave 14 CI regression gate workflow.
    ///
    /// This test documents the exact workflow that Wave 14 will implement:
    /// 1. Load baseline from artifact store (via `from_artifact_store`)
    /// 2. Run current benchmark
    /// 3. Query history for trends
    /// 4. Detect regressions
    /// 5. Emit alerts if threshold exceeded
    /// 6. Save new baseline for next run
    #[test]
    fn test_wave14_regression_gate_workflow() {
        let mut history_store =
            BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

        // Step 1: Simulate loading history from previous runs (Wave 14)
        // In real Wave 14, this would be: `from_artifact_store(&client, "benchmark-history-v1")`
        let historical_json_lines = r#"{"workload_id":"protocol-smoke-contract","commit_id":"abc001","timestamp":"2026-01-15T10:00:00Z","p50_latency_us":10000,"p95_latency_us":50000,"error_count":0,"sample_count":20,"branch":"main","pr_number":null}
{"workload_id":"protocol-smoke-contract","commit_id":"abc002","timestamp":"2026-01-15T10:10:00Z","p50_latency_us":10100,"p95_latency_us":50200,"error_count":0,"sample_count":20,"branch":"main","pr_number":null}
{"workload_id":"protocol-smoke-contract","commit_id":"abc003","timestamp":"2026-01-15T10:20:00Z","p50_latency_us":10200,"p95_latency_us":50400,"error_count":0,"sample_count":20,"branch":"main","pr_number":null}"#;

        history_store
            .import_json_lines(historical_json_lines)
            .unwrap();

        // Step 2: Run current benchmark (simulated)
        let current_record = BenchmarkHistoryRecord::new(
            "protocol-smoke-contract".to_string(),
            "abc004".to_string(),
            "2026-01-15T10:30:00Z".to_string(),
            10_500, // 5% degradation from trend
            51_000, // 2% degradation from trend
            0,
            20,
        )
        .with_context(Some("main".to_string()), Some(99));

        history_store.append(current_record.clone()).unwrap();

        // Step 3: Query history for regression analysis
        let result = history_store
            .query("protocol-smoke-contract", HistoryQuery::LastNCommits(4))
            .unwrap();

        assert_eq!(result.records.len(), 4);

        // Step 4: Detect regression using Wave 13 baseline
        // (In Wave 14, this would integrate with RegressionAnalysis from regression_detection.rs)
        let baseline_p50 = 10_000;
        let baseline_p95 = 50_000;
        let is_regressed = current_record.is_regressed_vs_baseline(baseline_p50, baseline_p95, 2.5);

        // 5% degradation exceeds 2.5% threshold
        assert!(is_regressed);

        // Step 5: Check if trend is getting worse
        // If this commit is in a series of degradations, flag as critical
        let regression_point = result.find_regression_point(2.0);
        assert!(regression_point.is_some()); // Regression detected in history

        // Step 6: Save updated history for next run (Wave 14)
        // In real Wave 14: `history_store.save_to_artifact_store(&client, "benchmark-history-v1")`
        let updated_json = history_store.save().unwrap();
        assert!(updated_json.contains("abc004")); // Current commit in history
    }

    /// Demonstrates empty store behavior.
    #[test]
    fn test_empty_store() {
        let store = BenchmarkHistoryStore::new(".andromeda/benchmark-history".to_string());

        assert_eq!(store.total_records(), 0);
        assert!(store.workload_ids().is_empty());

        let result = store.query("nonexistent", HistoryQuery::AllForWorkload);
        assert!(result.is_err());
    }

    /// Demonstrates that records with metadata (branch, PR number) are correctly preserved.
    #[test]
    fn test_record_metadata_preservation() {
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
}
