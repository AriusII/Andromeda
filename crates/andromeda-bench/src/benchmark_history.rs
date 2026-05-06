#![forbid(unsafe_code)]

//! Cross-commit benchmark history record model and query types.
//!
//! Records are immutable once created and are serialized to JSON Lines for
//! portable artifact storage. This module never modifies benchmark state —
//! it only models and queries history.
//!
//! ## Invariants
//!
//! - History records are append-only; no mutation after creation.
//! - Queries over the same record set with the same parameters are deterministic.
//! - All records are serializable to JSON Lines for artifact portability.

use crate::flat_json::{
    escape_json_string, optional_string, optional_u32, parse_flat_json_object, required_string,
    required_u64,
};

/// A single benchmark run record in history.
///
/// Captures all evidence from a benchmark run at a specific point in time (commit).
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkHistoryRecord {
    /// Workload identifier (e.g., "btree-lookup-smoke")
    pub workload_id: String,
    /// Commit hash or build ID identifying this run
    pub commit_id: String,
    /// ISO 8601 timestamp when benchmark was executed
    pub timestamp: String,
    /// P50 latency in microseconds
    pub p50_latency_us: u64,
    /// P95 latency in microseconds
    pub p95_latency_us: u64,
    /// Error count during benchmark
    pub error_count: u32,
    /// Total samples collected
    pub sample_count: u32,
    /// Git branch name (optional, for workflow context)
    pub branch: Option<String>,
    /// PR number if this was a PR check (optional)
    pub pr_number: Option<u32>,
}

impl BenchmarkHistoryRecord {
    pub fn new(
        workload_id: String,
        commit_id: String,
        timestamp: String,
        p50_latency_us: u64,
        p95_latency_us: u64,
        error_count: u32,
        sample_count: u32,
    ) -> Self {
        Self {
            workload_id,
            commit_id,
            timestamp,
            p50_latency_us,
            p95_latency_us,
            error_count,
            sample_count,
            branch: None,
            pr_number: None,
        }
    }

    /// Attach git context metadata to the record.
    pub fn with_context(mut self, branch: Option<String>, pr_number: Option<u32>) -> Self {
        self.branch = branch;
        self.pr_number = pr_number;
        self
    }

    /// Serialize record to JSON Line (one line, complete JSON object).
    ///
    /// JSON Lines format is chosen because it is portable across storage backends
    /// and supports append-without-parsing of the whole file.
    pub fn to_json_line(&self) -> String {
        format!(
            r#"{{"workload_id":"{}","commit_id":"{}","timestamp":"{}","p50_latency_us":{},"p95_latency_us":{},"error_count":{},"sample_count":{},"branch":{},"pr_number":{}}}"#,
            escape_json_string(&self.workload_id),
            escape_json_string(&self.commit_id),
            escape_json_string(&self.timestamp),
            self.p50_latency_us,
            self.p95_latency_us,
            self.error_count,
            self.sample_count,
            self.branch
                .as_ref()
                .map(|b| format!(r#""{}""#, escape_json_string(b)))
                .unwrap_or_else(|| "null".to_string()),
            self.pr_number
                .map(|p| p.to_string())
                .unwrap_or_else(|| "null".to_string())
        )
    }

    /// Deserialize from JSON Line format.
    pub fn from_json_line(line: &str) -> Result<Self, String> {
        let value = parse_flat_json_object(line)
            .map_err(|error| format!("invalid history JSON: {error}"))?;

        let workload_id = required_string(&value, "workload_id")?;
        let commit_id = required_string(&value, "commit_id")?;
        let timestamp = required_string(&value, "timestamp")?;
        let p50_latency_us = required_u64(&value, "p50_latency_us")?;
        let p95_latency_us = required_u64(&value, "p95_latency_us")?;
        let error_count = u32::try_from(required_u64(&value, "error_count")?)
            .map_err(|_| "error_count exceeds u32".to_string())?;
        let sample_count = u32::try_from(required_u64(&value, "sample_count")?)
            .map_err(|_| "sample_count exceeds u32".to_string())?;
        let branch = optional_string(&value, "branch")?;
        let pr_number = optional_u32(&value, "pr_number")?;

        Ok(Self {
            workload_id,
            commit_id,
            timestamp,
            p50_latency_us,
            p95_latency_us,
            error_count,
            sample_count,
            branch,
            pr_number,
        })
    }

    /// Returns true if P50 or P95 latency degraded more than `threshold_pct` relative to
    /// the provided baseline values.
    ///
    /// Regression is strict: exactly at the threshold is not considered regressed.
    pub fn is_regressed_vs_baseline(
        &self,
        baseline_p50_us: u64,
        baseline_p95_us: u64,
        threshold_pct: f64,
    ) -> bool {
        let p50_pct = percent_change(self.p50_latency_us, baseline_p50_us);
        let p95_pct = percent_change(self.p95_latency_us, baseline_p95_us);

        p50_pct > threshold_pct || p95_pct > threshold_pct
    }
}

/// Compute a finite percentage change from `baseline` to `current`.
///
/// A zero baseline has no mathematically meaningful percentage delta, but benchmark
/// histories still need deterministic regression/trend behavior. Treat a move from
/// zero to a positive value as a full finite regression and zero to zero as unchanged.
fn percent_change(current: u64, baseline: u64) -> f64 {
    match (current, baseline) {
        (0, 0) => 0.0,
        (_, 0) => 100.0,
        _ => ((current as f64 - baseline as f64) / baseline as f64) * 100.0,
    }
}

/// Range of timestamps for history queries.
#[derive(Debug, Clone)]
pub struct TimeRange {
    /// ISO 8601 start timestamp (inclusive)
    pub start_iso8601: String,
    /// ISO 8601 end timestamp (inclusive)
    pub end_iso8601: String,
}

impl TimeRange {
    pub fn new(start: String, end: String) -> Self {
        Self {
            start_iso8601: start,
            end_iso8601: end,
        }
    }

    /// Returns true if `timestamp` falls within the range (both bounds inclusive).
    ///
    /// Comparison is lexicographic over ISO 8601 strings, which is correct for
    /// well-formed UTC timestamps of equal length.
    pub fn contains(&self, timestamp: &str) -> bool {
        timestamp >= self.start_iso8601.as_str() && timestamp <= self.end_iso8601.as_str()
    }
}

/// Query parameters for history retrieval.
#[derive(Debug, Clone)]
pub enum HistoryQuery {
    /// Most recent N commits (chronological order in results)
    LastNCommits(usize),
    /// Commits whose timestamp falls within the given range
    TimeRange(TimeRange),
    /// Commits matching a specific commit ID
    CommitId(String),
    /// All commits for the workload
    AllForWorkload,
}

/// Results from a history query.
#[derive(Debug, Clone)]
pub struct HistoryQueryResult {
    pub workload_id: String,
    /// Records matching the query, in chronological order
    pub records: Vec<BenchmarkHistoryRecord>,
    /// Total records in the store for this workload (for caller context)
    pub total_in_store: usize,
}

impl HistoryQueryResult {
    /// Returns overall P50 trend as a percentage (positive = degradation, negative = improvement).
    ///
    /// Requires at least two records; returns `None` otherwise.
    pub fn trend_p50(&self) -> Option<f64> {
        if self.records.len() < 2 {
            return None;
        }
        let first = self.records.first()?.p50_latency_us;
        let last = self.records.last()?.p50_latency_us;
        Some(percent_change(last, first))
    }

    /// Returns overall P95 trend as a percentage.
    ///
    /// Requires at least two records; returns `None` otherwise.
    pub fn trend_p95(&self) -> Option<f64> {
        if self.records.len() < 2 {
            return None;
        }
        let first = self.records.first()?.p95_latency_us;
        let last = self.records.last()?.p95_latency_us;
        Some(percent_change(last, first))
    }

    /// Returns the commit ID and regression percentage of the single largest P50 degradation step
    /// that exceeds `threshold_pct`, or `None` if no step crosses the threshold.
    pub fn find_regression_point(&self, threshold_pct: f64) -> Option<(String, f64)> {
        if self.records.len() < 2 {
            return None;
        }

        let mut max_regression = 0.0;
        let mut regression_commit = None;

        for pair in self.records.windows(2) {
            let current = &pair[0];
            let next = &pair[1];

            let p50_regression = percent_change(next.p50_latency_us, current.p50_latency_us);

            if p50_regression > threshold_pct && p50_regression > max_regression {
                max_regression = p50_regression;
                regression_commit = Some(next.commit_id.clone());
            }
        }

        regression_commit.map(|commit| (commit, max_regression))
    }

    /// Returns true if every consecutive P50 step improved by more than `threshold_pct`.
    pub fn is_improving(&self, threshold_pct: f64) -> bool {
        if self.records.len() < 2 {
            return false;
        }

        for pair in self.records.windows(2) {
            let current = &pair[0];
            let next = &pair[1];

            let p50_change = percent_change(next.p50_latency_us, current.p50_latency_us);

            if p50_change > -threshold_pct {
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
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
    }

    #[test]
    fn test_regression_detection() {
        let record = BenchmarkHistoryRecord::new(
            "test".to_string(),
            "commit".to_string(),
            "2026-01-15T12:00:00Z".to_string(),
            105,
            525,
            0,
            10,
        );

        // Regression is strict: exactly at threshold is not regressed.
        assert!(!record.is_regressed_vs_baseline(100, 500, 5.0));
        assert!(record.is_regressed_vs_baseline(100, 500, 4.0));
    }

    #[test]
    fn test_regression_detection_with_zero_baseline_is_finite() {
        let unchanged_zero = BenchmarkHistoryRecord::new(
            "test".to_string(),
            "commit".to_string(),
            "2026-01-15T12:00:00Z".to_string(),
            0,
            0,
            0,
            10,
        );

        assert!(!unchanged_zero.is_regressed_vs_baseline(0, 0, 2.5));

        let nonzero_current = BenchmarkHistoryRecord::new(
            "test".to_string(),
            "commit".to_string(),
            "2026-01-15T12:00:00Z".to_string(),
            1,
            0,
            0,
            10,
        );

        assert!(nonzero_current.is_regressed_vs_baseline(0, 0, 2.5));
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
                130, // ~27% regression vs c2
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
}
