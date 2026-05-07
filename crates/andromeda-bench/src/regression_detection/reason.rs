/// Reason why a workload regressed (or didn't).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegressionReason {
    /// P50 latency increased beyond baseline
    P50Degradation,
    /// P95 latency increased beyond baseline
    P95Degradation,
    /// Error rate increased
    ErrorRateIncrease,
    /// Multiple metrics degraded
    MultipleMetrics,
    /// No regression detected
    NoRegression,
}

impl RegressionReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::P50Degradation => "p50-latency-degradation",
            Self::P95Degradation => "p95-latency-degradation",
            Self::ErrorRateIncrease => "error-rate-increase",
            Self::MultipleMetrics => "multiple-metrics",
            Self::NoRegression => "no-regression",
        }
    }
}
