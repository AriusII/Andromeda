use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use std::sync::Arc;
use std::time::Duration;

use super::{
    CompactionContext, WalCompactionAuditEvent, WalCompactionSummary, compact_segment,
    identify_compaction_candidates,
};

/// Configuration for the WAL compaction scheduler.
#[derive(Debug, Clone)]
pub struct WalCompactionSchedulerConfig {
    /// Interval between compaction runs
    pub interval: Duration,
    /// Fragmentation threshold; compact segments with ratio >= this value (e.g., 0.30)
    pub fragmentation_threshold: f64,
    /// Maximum segments to compact per run (0 = unlimited)
    pub max_segments_per_run: u64,
}

impl WalCompactionSchedulerConfig {
    /// Create a new compaction scheduler configuration.
    pub fn new(interval: Duration, fragmentation_threshold: f64) -> AndromedaResult<Self> {
        if interval.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL compaction: scheduler interval must not be zero",
            ));
        }
        if !(0.0..=1.0).contains(&fragmentation_threshold) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL compaction: fragmentation threshold must be in [0.0, 1.0]",
            ));
        }

        Ok(WalCompactionSchedulerConfig {
            interval,
            fragmentation_threshold,
            max_segments_per_run: 0, // No limit by default
        })
    }

    /// Set maximum segments to compact per run.
    pub fn with_max_segments(mut self, max: u64) -> Self {
        self.max_segments_per_run = max;
        self
    }
}

/// Background scheduler for WAL segment compaction.
///
/// Periodically identifies fragmented segments and compacts up to N per run.
/// Emits metrics for observability.
#[derive(Debug)]
pub struct WalCompactionScheduler {
    config: WalCompactionSchedulerConfig,
    run_counter: Arc<std::sync::atomic::AtomicU64>,
}

impl WalCompactionScheduler {
    /// Create a new compaction scheduler.
    pub fn new(config: WalCompactionSchedulerConfig) -> Self {
        WalCompactionScheduler {
            config,
            run_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Execute a single compaction run.
    ///
    /// # Algorithm
    ///
    /// 1. Identify candidates exceeding fragmentation threshold
    /// 2. Sort by fragmentation_ratio (highest first)
    /// 3. Compact up to max_segments_per_run
    /// 4. Collect metrics and emit summary
    ///
    /// # Returns
    ///
    /// `WalCompactionSummary` with metrics from this run.
    pub fn run(&self, context: &dyn CompactionContext) -> AndromedaResult<WalCompactionSummary> {
        let run_id = self
            .run_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let mut summary = WalCompactionSummary::new(run_id);

        // Step 1: Identify candidates
        let candidates =
            identify_compaction_candidates(context, self.config.fragmentation_threshold)?;
        summary.candidates_identified = candidates.len() as u64;

        // Step 2: Compact up to max_segments_per_run
        let limit = if self.config.max_segments_per_run == 0 {
            candidates.len()
        } else {
            (self.config.max_segments_per_run as usize).min(candidates.len())
        };

        for metrics in candidates.iter().take(limit) {
            match compact_segment(context, metrics.segment_id) {
                Ok(result) => {
                    summary.candidates_compacted += 1;
                    summary.total_bytes_recovered += result.bytes_recovered;
                },
                Err(_) => {
                    summary.compaction_skipped += 1;
                },
            }
        }

        // Step 3: Emit summary
        let avg_ratio = if summary.candidates_compacted > 0 {
            summary.total_bytes_recovered as f64 / (summary.candidates_compacted as f64 * 1000.0)
        } else {
            0.0
        };

        context.emit_audit_event(WalCompactionAuditEvent::Summary {
            run_id,
            candidates_identified: summary.candidates_identified,
            candidates_compacted: summary.candidates_compacted,
            compaction_skipped: summary.compaction_skipped,
            total_bytes_recovered: summary.total_bytes_recovered,
            total_reduction_ratio: avg_ratio,
        })?;

        Ok(summary)
    }

    /// Get the interval between compaction runs.
    pub const fn interval(&self) -> Duration {
        self.config.interval
    }

    /// Get the fragmentation threshold.
    pub const fn fragmentation_threshold(&self) -> f64 {
        self.config.fragmentation_threshold
    }
}
