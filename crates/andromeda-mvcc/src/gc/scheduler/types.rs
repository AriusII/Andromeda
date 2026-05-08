/// Lifecycle exit reason for a scheduler loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcSchedulerExitReason {
    /// The caller's shutdown signal completed.
    ShutdownRequested,
}

/// Snapshot of scheduler-local lifecycle evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcSchedulerStats {
    /// Number of interval ticks observed by the scheduler loop.
    pub ticks: u64,
    /// Number of ticks that invoked a GC pass.
    pub gc_runs_started: u64,
    /// Number of GC passes that completed successfully.
    pub gc_runs_completed: u64,
    /// Number of ticks skipped because the minimum visible timestamp did not
    /// change.
    pub skipped_no_min_visible_change: u64,
    /// Number of GC pass errors observed by the scheduler.
    pub errors: u64,
    /// Last minimum visible timestamp observed by the scheduler.
    pub last_observed_min_visible_ts: u64,
}

/// Evidence returned when a scheduler loop exits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcSchedulerExit {
    pub reason: GcSchedulerExitReason,
    pub stats: GcSchedulerStats,
}
