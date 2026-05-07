use super::record::BenchmarkHistoryRecord;
use crate::metric_math::percent_change;

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
        self.trend_by_latency(|record| record.p50_latency_us)
    }

    /// Returns overall P95 trend as a percentage.
    ///
    /// Requires at least two records; returns `None` otherwise.
    pub fn trend_p95(&self) -> Option<f64> {
        self.trend_by_latency(|record| record.p95_latency_us)
    }

    fn trend_by_latency(&self, latency: impl Fn(&BenchmarkHistoryRecord) -> u64) -> Option<f64> {
        if self.records.len() < 2 {
            return None;
        }
        let first = latency(self.records.first()?);
        let last = latency(self.records.last()?);
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
