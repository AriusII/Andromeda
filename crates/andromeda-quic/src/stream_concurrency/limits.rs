use std::time::Duration;

/// Bounded stream concurrency and timeout configuration for one QUIC connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamConcurrencyLimits {
    max_concurrent: usize,
    idle_timeout: Duration,
    overall_timeout: Duration,
}

impl StreamConcurrencyLimits {
    /// Default maximum concurrent streams per connection.
    pub const DEFAULT_MAX_CONCURRENT: usize = 128;
    /// Default idle timeout duration (30 seconds).
    pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
    /// Default overall timeout duration (5 minutes).
    pub const DEFAULT_OVERALL_TIMEOUT: Duration = Duration::from_secs(300);

    /// Creates stream limits from explicit bounded values.
    pub const fn new(
        max_concurrent: usize,
        idle_timeout: Duration,
        overall_timeout: Duration,
    ) -> Self {
        Self {
            max_concurrent,
            idle_timeout,
            overall_timeout,
        }
    }

    pub const fn max_concurrent(self) -> usize {
        self.max_concurrent
    }

    pub const fn idle_timeout(self) -> Duration {
        self.idle_timeout
    }

    pub const fn overall_timeout(self) -> Duration {
        self.overall_timeout
    }
}

impl Default for StreamConcurrencyLimits {
    fn default() -> Self {
        Self::new(
            Self::DEFAULT_MAX_CONCURRENT,
            Self::DEFAULT_IDLE_TIMEOUT,
            Self::DEFAULT_OVERALL_TIMEOUT,
        )
    }
}
