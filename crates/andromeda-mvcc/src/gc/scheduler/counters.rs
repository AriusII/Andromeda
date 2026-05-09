use std::sync::atomic::AtomicU64;

#[derive(Debug)]
pub(super) struct GcSchedulerCounters {
    pub(super) ticks: AtomicU64,
    pub(super) gc_runs_started: AtomicU64,
    pub(super) gc_runs_completed: AtomicU64,
    pub(super) skipped_no_min_visible_change: AtomicU64,
    pub(super) errors: AtomicU64,
}

impl GcSchedulerCounters {
    pub(super) fn new() -> Self {
        Self {
            ticks: AtomicU64::new(0),
            gc_runs_started: AtomicU64::new(0),
            gc_runs_completed: AtomicU64::new(0),
            skipped_no_min_visible_change: AtomicU64::new(0),
            errors: AtomicU64::new(0),
        }
    }
}
