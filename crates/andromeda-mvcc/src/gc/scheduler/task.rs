use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::gc::MvccGarbageCollector;

use super::counters::GcSchedulerCounters;
use super::{
    GcSchedulerExit, GcSchedulerExitReason, GcSchedulerHandle, GcSchedulerStats,
    MIN_GC_SCHEDULER_INTERVAL,
};

/// Background task for periodic garbage collection.
pub struct GcSchedulerTask {
    collector: Arc<MvccGarbageCollector>,
    run_interval: Duration,
    last_min_visible_ts: AtomicU64,
    counters: GcSchedulerCounters,
}

impl GcSchedulerTask {
    /// Create a new GC scheduler task.
    ///
    /// # Arguments
    ///
    /// * `collector` - The garbage collector to invoke
    /// * `run_interval` - How often to attempt a GC run (e.g., Duration::from_millis(100))
    pub fn new(collector: Arc<MvccGarbageCollector>, run_interval: Duration) -> Self {
        let run_interval = if run_interval.is_zero() {
            MIN_GC_SCHEDULER_INTERVAL
        } else {
            run_interval
        };

        Self {
            collector,
            run_interval,
            last_min_visible_ts: AtomicU64::new(0),
            counters: GcSchedulerCounters::new(),
        }
    }

    /// Create a scheduler and reject an unbounded zero-duration interval.
    pub fn try_new(
        collector: Arc<MvccGarbageCollector>,
        run_interval: Duration,
    ) -> AndromedaResult<Self> {
        if run_interval.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "gc scheduler interval must be greater than zero",
            ));
        }

        Ok(Self::new(collector, run_interval))
    }

    /// Return the effective tick interval.
    pub fn run_interval(&self) -> Duration {
        self.run_interval
    }

    /// Return scheduler-local lifecycle evidence.
    pub fn scheduler_stats(&self) -> GcSchedulerStats {
        GcSchedulerStats {
            ticks: self.counters.ticks.load(Ordering::Relaxed),
            gc_runs_started: self.counters.gc_runs_started.load(Ordering::Relaxed),
            gc_runs_completed: self.counters.gc_runs_completed.load(Ordering::Relaxed),
            skipped_no_min_visible_change: self
                .counters
                .skipped_no_min_visible_change
                .load(Ordering::Relaxed),
            errors: self.counters.errors.load(Ordering::Relaxed),
            last_observed_min_visible_ts: self.last_min_visible_ts.load(Ordering::Relaxed),
        }
    }

    /// Spawn this scheduler on the current Tokio runtime.
    ///
    /// The returned handle owns cancellation and join authority. This method
    /// does not integrate with transaction begin/commit/rollback paths; engine
    /// owners must opt into the background task explicitly.
    pub fn start(self: Arc<Self>) -> GcSchedulerHandle {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let join = tokio::spawn(async move {
            self.run_until_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
        });

        GcSchedulerHandle {
            shutdown_tx: Some(shutdown_tx),
            join: Some(join),
        }
    }

    /// Execute at most one scheduler tick.
    ///
    /// This deterministic contract is the unit of background work: observe the
    /// current MVCC visibility frontier, and run GC only if the frontier changed
    /// since the last successful scheduler observation. It never loops, never
    /// sleeps, never touches WAL, and never publishes commit/rollback visibility.
    pub fn tick_once(&self) -> AndromedaResult<Option<crate::gc::GcSummary>> {
        self.counters.ticks.fetch_add(1, Ordering::Relaxed);

        let current_min_visible = self.collector.minimum_visible_timestamp();
        let last_min_visible = self.last_min_visible_ts.load(Ordering::Relaxed);

        // Only run GC if min_visible_ts changed. This preserves the active
        // snapshot invariant because all eligibility decisions still flow
        // through MvccGarbageCollector and ActiveSnapshotRegistry.
        if current_min_visible != last_min_visible {
            self.counters
                .gc_runs_started
                .fetch_add(1, Ordering::Relaxed);
            match self.collector.run_gc() {
                Ok(summary) => {
                    self.last_min_visible_ts
                        .store(current_min_visible, Ordering::Relaxed);
                    self.counters
                        .gc_runs_completed
                        .fetch_add(1, Ordering::Relaxed);
                    Ok(Some(summary))
                }
                Err(e) => {
                    self.counters.errors.fetch_add(1, Ordering::Relaxed);
                    Err(e)
                }
            }
        } else {
            self.counters
                .skipped_no_min_visible_change
                .fetch_add(1, Ordering::Relaxed);
            Ok(None)
        }
    }

    /// Run the periodic GC loop until `shutdown` completes.
    ///
    /// Cancellation is cooperative and bounded by `run_interval` plus the
    /// duration of a single `run_gc` call. A shutdown signal received while the
    /// task is sleeping exits without starting another GC run.
    pub async fn run_until_shutdown<F>(&self, shutdown: F) -> AndromedaResult<GcSchedulerExit>
    where
        F: Future<Output = ()>,
    {
        tokio::pin!(shutdown);

        loop {
            tokio::select! {
                biased;
                _ = &mut shutdown => {
                    return Ok(GcSchedulerExit {
                        reason: GcSchedulerExitReason::ShutdownRequested,
                        stats: self.scheduler_stats(),
                    });
                }
                _ = tokio::time::sleep(self.run_interval) => {
                    if let Err(e) = self.tick_once() {
                        // Record-and-continue policy: scheduler errors must not
                        // poison transaction state or stop future cleanup attempts.
                        eprintln!("GC scheduler error: {:?}", e);
                    }
                }
            }
        }
    }

    /// Run the periodic GC loop.
    ///
    /// This is an infinite loop that:
    /// 1. Sleeps for `run_interval`
    /// 2. Checks if GC should run (min_visible_ts changed, etc.)
    /// 3. Invokes the garbage collector
    /// 4. Repeats
    ///
    /// Designed to be spawned in a tokio::spawn() context.
    /// Errors are logged but do not stop the loop.
    pub async fn run_periodic_gc(&self) -> AndromedaResult<()> {
        let _never_exits = self.run_until_shutdown(std::future::pending()).await?;
        Ok(())
    }
}
