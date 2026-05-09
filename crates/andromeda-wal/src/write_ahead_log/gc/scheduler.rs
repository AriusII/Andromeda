use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use std::sync::Arc;
use std::time::Duration;

use super::super::scheduler_config::{UNLIMITED_SEGMENTS_PER_RUN, validate_nonzero_interval};
use super::{WalGarbageCollector, WalGcSummary};

/// Configuration for the WAL GC scheduler.
#[derive(Debug, Clone)]
pub struct WalGcSchedulerConfig {
    /// Interval between GC runs.
    pub interval: Duration,
    /// Target free space in GiB; trigger GC when HotStore free < this value.
    pub target_free_gib: u64,
    /// Maximum segments to process per run (0 = unlimited).
    pub max_segments_per_run: u64,
}

impl WalGcSchedulerConfig {
    pub fn new(interval: Duration, target_free_gib: u64) -> Self {
        WalGcSchedulerConfig {
            interval,
            target_free_gib,
            max_segments_per_run: UNLIMITED_SEGMENTS_PER_RUN,
        }
    }

    pub fn with_max_segments(mut self, max: u64) -> Self {
        self.max_segments_per_run = max;
        self
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        validate_nonzero_interval(self.interval, "WAL GC scheduler interval must not be zero")?;
        if self.target_free_gib == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL GC scheduler target free GiB must be greater than zero",
            ));
        }
        Ok(())
    }
}

/// WAL Garbage Collection scheduler.
pub struct WalGcScheduler {
    config: WalGcSchedulerConfig,
    gc: Arc<WalGarbageCollector>,
}

impl WalGcScheduler {
    /// Create a new scheduler with the given configuration and GC context.
    pub fn new(
        config: WalGcSchedulerConfig,
        gc: Arc<WalGarbageCollector>,
    ) -> AndromedaResult<Self> {
        config.validate()?;
        Ok(WalGcScheduler { config, gc })
    }

    /// Get the scheduler configuration.
    pub fn config(&self) -> &WalGcSchedulerConfig {
        &self.config
    }

    /// Execute a single scheduled GC run.
    pub fn tick(&self, run_id: u64) -> AndromedaResult<WalGcSummary> {
        self.gc.run_gc(run_id)
    }
}
