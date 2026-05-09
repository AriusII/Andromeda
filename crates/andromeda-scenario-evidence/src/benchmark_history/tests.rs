use crate::{MAX_DURATION_MS, MAX_SAMPLES, MAX_TEMP_BYTES};

use super::*;

#[test]
fn test_history_record_creation() {
    let record = BenchmarkHistoryRecord::new(
        "btree-lookup-smoke".to_string(),
        "abc123def456".to_string(),
        "2026-01-15T14:30:45Z".to_string(),
        25,
        500,
        0,
        20,
    );

    assert_eq!(record.workload_id, "btree-lookup-smoke");
    assert_eq!(record.commit_id, "abc123def456");
    assert_eq!(record.p50_latency_us, 25);
    assert_eq!(record.error_count, 0);
}

#[test]
fn test_history_record_json_serialization() {
    let record = BenchmarkHistoryRecord::new(
        "test-workload".to_string(),
        "commit123".to_string(),
        "2026-01-15T12:00:00Z".to_string(),
        100,
        500,
        0,
        10,
    )
    .with_context(Some("main".to_string()), Some(42));

    let json_line = record.to_json_line();
    let deserialized = BenchmarkHistoryRecord::from_json_line(&json_line).unwrap();

    assert_eq!(record, deserialized);
    assert_eq!(deserialized.branch, Some("main".to_string()));
    assert_eq!(deserialized.pr_number, Some(42));
    assert!(json_line.contains(r#""advisory_boundary":"advisory-only""#));
    assert!(json_line.contains(r#""authoritative":false"#));
    assert!(json_line.contains(r#""can_select_plan_alone":false"#));
    assert!(json_line.contains(r#""optimizer_boundary":"advisory-only""#));
    assert!(json_line.contains(r#""duration_cap_ms":"#));
    assert!(json_line.contains(r#""sample_cap":"#));
    assert!(json_line.contains(r#""temp_cap_bytes":"#));
    assert!(!deserialized.advisory.authoritative);
    assert!(!deserialized.advisory.can_select_plan_alone);
    assert_eq!(deserialized.advisory.optimizer_boundary, "advisory-only");
}

#[test]
fn test_history_record_imports_legacy_json_with_advisory_defaults() {
    let legacy_json_line = r#"{"workload_id":"test-workload","commit_id":"commit123","timestamp":"2026-01-15T12:00:00Z","p50_latency_us":100,"p95_latency_us":500,"error_count":0,"sample_count":10,"branch":null,"pr_number":null}"#;

    let record = BenchmarkHistoryRecord::from_json_line(legacy_json_line).unwrap();

    assert_eq!(record.workload_id, "test-workload");
    assert_eq!(record.advisory.advisory_boundary, "advisory-only");
    assert!(!record.advisory.authoritative);
    assert!(!record.advisory.can_select_plan_alone);
    assert_eq!(record.advisory.optimizer_boundary, "advisory-only");
    assert_eq!(record.advisory.duration_cap_ms, MAX_DURATION_MS);
    assert_eq!(record.advisory.sample_cap, MAX_SAMPLES);
    assert_eq!(record.advisory.temp_cap_bytes, MAX_TEMP_BYTES);
}

#[test]
fn test_time_range() {
    let range = TimeRange::new(
        "2026-01-15T00:00:00Z".to_string(),
        "2026-01-16T00:00:00Z".to_string(),
    );

    assert!(range.contains("2026-01-15T12:00:00Z"));
    assert!(range.contains("2026-01-15T00:00:00Z"));
    assert!(range.contains("2026-01-16T00:00:00Z"));
    assert!(!range.contains("2026-01-14T23:59:59Z"));
    assert!(!range.contains("2026-01-16T00:00:01Z"));
}

#[test]
fn test_query_result_trend_calculation() {
    let records = vec![
        BenchmarkHistoryRecord::new(
            "test".to_string(),
            "c1".to_string(),
            "2026-01-15T12:00:00Z".to_string(),
            100,
            500,
            0,
            10,
        ),
        BenchmarkHistoryRecord::new(
            "test".to_string(),
            "c2".to_string(),
            "2026-01-15T13:00:00Z".to_string(),
            105,
            510,
            0,
            10,
        ),
        BenchmarkHistoryRecord::new(
            "test".to_string(),
            "c3".to_string(),
            "2026-01-15T14:00:00Z".to_string(),
            110,
            520,
            0,
            10,
        ),
    ];

    let result = HistoryQueryResult {
        workload_id: "test".to_string(),
        records: records.clone(),
        total_in_store: 3,
    };

    let trend_p50 = result.trend_p50().unwrap();
    assert!((trend_p50 - 10.0).abs() < 0.01);
}

#[test]
fn test_query_result_zero_baseline_trends_are_finite() {
    let records = vec![
        BenchmarkHistoryRecord::new(
            "test".to_string(),
            "c1".to_string(),
            "2026-01-15T12:00:00Z".to_string(),
            0,
            0,
            0,
            10,
        ),
        BenchmarkHistoryRecord::new(
            "test".to_string(),
            "c2".to_string(),
            "2026-01-15T13:00:00Z".to_string(),
            10,
            0,
            0,
            10,
        ),
    ];

    let result = HistoryQueryResult {
        workload_id: "test".to_string(),
        records,
        total_in_store: 2,
    };

    let trend_p50 = result.trend_p50().unwrap();
    let trend_p95 = result.trend_p95().unwrap();

    assert!(trend_p50.is_finite());
    assert!(trend_p95.is_finite());
    assert_eq!(trend_p50, 100.0);
    assert_eq!(trend_p95, 0.0);
}

#[test]
fn test_find_regression_point() {
    let records = vec![
        BenchmarkHistoryRecord::new(
            "test".to_string(),
            "c1".to_string(),
            "2026-01-15T12:00:00Z".to_string(),
            100,
            500,
            0,
            10,
        ),
        BenchmarkHistoryRecord::new(
            "test".to_string(),
            "c2".to_string(),
            "2026-01-15T13:00:00Z".to_string(),
            102,
            510,
            0,
            10,
        ),
        BenchmarkHistoryRecord::new(
            "test".to_string(),
            "c3".to_string(),
            "2026-01-15T14:00:00Z".to_string(),
            130,
            550,
            0,
            10,
        ),
    ];

    let result = HistoryQueryResult {
        workload_id: "test".to_string(),
        records: records.clone(),
        total_in_store: 3,
    };

    let regression = result.find_regression_point(5.0).unwrap();
    assert_eq!(regression.0, "c3");
    assert!(regression.1 > 25.0);
}

#[test]
fn test_find_regression_point_with_zero_previous_value_is_finite() {
    let records = vec![
        BenchmarkHistoryRecord::new(
            "test".to_string(),
            "c1".to_string(),
            "2026-01-15T12:00:00Z".to_string(),
            0,
            500,
            0,
            10,
        ),
        BenchmarkHistoryRecord::new(
            "test".to_string(),
            "c2".to_string(),
            "2026-01-15T13:00:00Z".to_string(),
            1,
            510,
            0,
            10,
        ),
    ];

    let result = HistoryQueryResult {
        workload_id: "test".to_string(),
        records,
        total_in_store: 2,
    };

    let regression = result.find_regression_point(5.0).unwrap();
    assert_eq!(regression.0, "c2");
    assert_eq!(regression.1, 100.0);
    assert!(regression.1.is_finite());
}

#[test]
fn test_empty_query_result() {
    let result = HistoryQueryResult {
        workload_id: "test".to_string(),
        records: vec![],
        total_in_store: 0,
    };

    assert_eq!(result.trend_p50(), None);
    assert_eq!(result.find_regression_point(5.0), None);
}
