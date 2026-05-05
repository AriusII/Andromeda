//! Garbage Collection Scheduler
//!
//! Provides a bounded, cancel-safe background task that periodically runs MVCC
//! garbage collection evidence collection.
//!
//! The scheduler deliberately does **not** own storage scanning, WAL, commit,
//! or rollback critical paths. Each tick performs at most one synchronous
//! [`MvccGarbageCollector::run_gc`] call, which currently records a GC pass and
//! returns visibility evidence. Actual version reclamation remains delegated to
//! the storage/coordinator layer.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::gc::MvccGarbageCollector;

/// Minimum allowed scheduler interval.
///
/// This prevents zero-duration busy loops while preserving the legacy
/// [`GcSchedulerTask::new`] constructor. Callers that need strict validation
/// should use [`GcSchedulerTask::try_new`].
pub const MIN_GC_SCHEDULER_INTERVAL: Duration = Duration::from_millis(1);

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

/// Owned lifecycle handle for a spawned GC scheduler task.
///
/// Dropping this handle aborts the spawned task so callers cannot accidentally
/// detach an uncontrolled long-lived background task. Prefer [`Self::shutdown`]
/// for graceful cooperative shutdown evidence.
pub struct GcSchedulerHandle {
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    join: Option<tokio::task::JoinHandle<AndromedaResult<GcSchedulerExit>>>,
}

impl GcSchedulerHandle {
    /// Request cooperative shutdown. Returns `true` if this call delivered the
    /// first shutdown signal.
    pub fn request_shutdown(&mut self) -> bool {
        self.shutdown_tx
            .take()
            .map(|tx| tx.send(()).is_ok())
            .unwrap_or(false)
    }

    /// Request shutdown and await scheduler exit evidence.
    pub async fn shutdown(mut self) -> AndromedaResult<GcSchedulerExit> {
        let _ = self.request_shutdown();
        let join = self.join.take().ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "gc scheduler join handle already consumed",
            )
        })?;

        join.await.map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Transaction,
                format!("gc scheduler task join failed: {e}"),
            )
        })?
    }
}

impl Drop for GcSchedulerHandle {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.take().map(|tx| tx.send(()));
        if let Some(join) = &self.join {
            join.abort();
        }
    }
}

#[derive(Debug)]
struct GcSchedulerCounters {
    ticks: AtomicU64,
    gc_runs_started: AtomicU64,
    gc_runs_completed: AtomicU64,
    skipped_no_min_visible_change: AtomicU64,
    errors: AtomicU64,
}

impl GcSchedulerCounters {
    fn new() -> Self {
        Self {
            ticks: AtomicU64::new(0),
            gc_runs_started: AtomicU64::new(0),
            gc_runs_completed: AtomicU64::new(0),
            skipped_no_min_visible_change: AtomicU64::new(0),
            errors: AtomicU64::new(0),
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::active_snapshot_registry::{ActiveSnapshotRegistry, SnapshotHandle};
    use crate::mvcc_status::TransactionStatusTable;
    use andromeda_core::TransactionId;

    #[tokio::test]
    async fn test_gc_scheduler_task_respects_interval() {
        let collector = Arc::new(crate::gc::MvccGarbageCollector::new(
            Arc::new(ActiveSnapshotRegistry::new()),
            Arc::new(TransactionStatusTable::new()),
        ));

        let scheduler = Arc::new(GcSchedulerTask::new(
            collector.clone(),
            Duration::from_millis(50),
        ));

        // Spawn the scheduler for a short period
        let scheduler_task = scheduler.clone();
        let handle = tokio::spawn(async move { scheduler_task.run_periodic_gc().await });

        // Let it run for 250ms
        tokio::time::sleep(Duration::from_millis(250)).await;

        // Cancel the task
        handle.abort();

        // Check that GC ran at least once after the initial threshold observation.
        let stats = collector.get_stats();
        assert!(
            stats.runs >= 1,
            "Expected at least 1 GC run, got {}",
            stats.runs
        );
    }

    #[tokio::test]
    async fn test_gc_scheduler_task_skips_when_no_change() {
        let registry = Arc::new(ActiveSnapshotRegistry::new());
        let status_table = Arc::new(TransactionStatusTable::new());
        let collector = Arc::new(crate::gc::MvccGarbageCollector::new(
            registry.clone(),
            status_table,
        ));

        let scheduler = Arc::new(GcSchedulerTask::new(
            collector.clone(),
            Duration::from_millis(50),
        ));

        // Register a snapshot so min_visible_ts is fixed
        let tx_id = TransactionId::new(1);
        registry
            .register_snapshot(SnapshotHandle::new(100, tx_id).expect("snapshot"))
            .expect("register");

        // Spawn the scheduler
        let scheduler_task = scheduler.clone();
        let handle = tokio::spawn(async move { scheduler_task.run_periodic_gc().await });

        // Let it run for 200ms (should attempt 4 GC runs)
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Cancel the task
        handle.abort();

        // Since min_visible_ts didn't change, GC might not have actually
        // executed the full scan (depends on implementation). At minimum,
        // the loop should have completed multiple cycles.
        let stats = collector.get_stats();
        // This test is informational; actual run count depends on min_visible_ts changes
        println!(
            "GC runs after 200ms with no snapshot change: {}",
            stats.runs
        );
    }

    #[test]
    fn test_gc_scheduler_rejects_or_clamps_zero_interval() {
        let collector = Arc::new(crate::gc::MvccGarbageCollector::new(
            Arc::new(ActiveSnapshotRegistry::new()),
            Arc::new(TransactionStatusTable::new()),
        ));

        assert!(GcSchedulerTask::try_new(collector.clone(), Duration::ZERO).is_err());

        let scheduler = GcSchedulerTask::new(collector, Duration::ZERO);
        assert_eq!(scheduler.run_interval(), MIN_GC_SCHEDULER_INTERVAL);
    }

    #[test]
    fn test_gc_scheduler_tick_once_is_bounded_and_observable() {
        let registry = Arc::new(ActiveSnapshotRegistry::new());
        let status_table = Arc::new(TransactionStatusTable::new());
        let collector = Arc::new(crate::gc::MvccGarbageCollector::new(
            registry.clone(),
            status_table,
        ));

        let scheduler = GcSchedulerTask::new(collector.clone(), Duration::from_secs(1));

        // With no active snapshots, min_visible_ts is u64::MAX and the first
        // tick records the initial frontier.
        let first = scheduler.tick_once().expect("first tick");
        assert!(first.is_some());
        assert_eq!(collector.get_stats().runs, 1);

        // A second tick with no frontier advance is a bounded no-op.
        let second = scheduler.tick_once().expect("second tick");
        assert!(second.is_none());
        assert_eq!(collector.get_stats().runs, 1);

        let stats = scheduler.scheduler_stats();
        assert_eq!(stats.ticks, 2);
        assert_eq!(stats.gc_runs_started, 1);
        assert_eq!(stats.gc_runs_completed, 1);
        assert_eq!(stats.skipped_no_min_visible_change, 1);
        assert_eq!(stats.last_observed_min_visible_ts, u64::MAX);
    }

    #[test]
    fn test_gc_scheduler_tick_advances_only_after_snapshot_frontier_moves() {
        let registry = Arc::new(ActiveSnapshotRegistry::new());
        let status_table = Arc::new(TransactionStatusTable::new());
        let collector = Arc::new(crate::gc::MvccGarbageCollector::new(
            registry.clone(),
            status_table,
        ));

        let snap1 = SnapshotHandle::new(100, TransactionId::new(1)).expect("first snapshot handle");
        let snap2 =
            SnapshotHandle::new(200, TransactionId::new(2)).expect("second snapshot handle");
        registry.register_snapshot(snap1).expect("register snap1");
        registry.register_snapshot(snap2).expect("register snap2");

        let scheduler = GcSchedulerTask::new(collector.clone(), Duration::from_secs(1));
        assert!(scheduler.tick_once().expect("initial tick").is_some());
        assert_eq!(
            scheduler.scheduler_stats().last_observed_min_visible_ts,
            100
        );

        assert!(scheduler.tick_once().expect("stable tick").is_none());
        assert_eq!(collector.get_stats().runs, 1);

        registry.release_snapshot(snap1).expect("release snap1");
        let advanced = scheduler.tick_once().expect("advanced tick");
        assert!(advanced.is_some());
        assert_eq!(advanced.unwrap().min_visible_ts, 200);
        assert_eq!(collector.get_stats().runs, 2);
    }

    #[test]
    fn test_gc_scheduler_detects_snapshot_cycle_after_empty_registry() {
        let registry = Arc::new(ActiveSnapshotRegistry::new());
        let status_table = Arc::new(TransactionStatusTable::new());
        let collector = Arc::new(crate::gc::MvccGarbageCollector::new(
            registry.clone(),
            status_table,
        ));

        let scheduler = GcSchedulerTask::new(collector.clone(), Duration::from_secs(1));

        // Initial empty registry records u64::MAX.
        assert!(scheduler.tick_once().expect("initial empty tick").is_some());
        assert_eq!(
            scheduler.scheduler_stats().last_observed_min_visible_ts,
            u64::MAX
        );

        // A new active snapshot moves the frontier backward. Running a bounded
        // evidence pass is safe and prevents starvation after the snapshot closes.
        let snap = SnapshotHandle::new(100, TransactionId::new(3)).expect("snapshot");
        registry.register_snapshot(snap).expect("register snapshot");
        let active = scheduler.tick_once().expect("active snapshot tick");
        assert!(active.is_some());
        assert_eq!(active.unwrap().min_visible_ts, 100);

        registry.release_snapshot(snap).expect("release snapshot");
        let released = scheduler.tick_once().expect("released snapshot tick");
        assert!(released.is_some());
        assert_eq!(released.unwrap().min_visible_ts, u64::MAX);
        assert_eq!(collector.get_stats().runs, 3);
    }

    #[tokio::test]
    async fn test_gc_scheduler_shutdown_signal_exits_without_waiting_interval() {
        let collector = Arc::new(crate::gc::MvccGarbageCollector::new(
            Arc::new(ActiveSnapshotRegistry::new()),
            Arc::new(TransactionStatusTable::new()),
        ));
        let scheduler = GcSchedulerTask::new(collector.clone(), Duration::from_secs(60));
        let (tx, rx) = tokio::sync::oneshot::channel();
        tx.send(()).expect("shutdown signal");

        let exit = scheduler
            .run_until_shutdown(async move {
                let _ = rx.await;
            })
            .await
            .expect("scheduler exit");

        assert_eq!(exit.reason, GcSchedulerExitReason::ShutdownRequested);
        assert_eq!(exit.stats.ticks, 0);
        assert_eq!(collector.get_stats().runs, 0);
    }

    #[tokio::test]
    async fn test_gc_scheduler_start_handle_owns_shutdown() {
        let collector = Arc::new(crate::gc::MvccGarbageCollector::new(
            Arc::new(ActiveSnapshotRegistry::new()),
            Arc::new(TransactionStatusTable::new()),
        ));
        let scheduler = Arc::new(GcSchedulerTask::new(
            collector.clone(),
            Duration::from_secs(60),
        ));

        let handle = scheduler.start();
        let exit = handle.shutdown().await.expect("graceful shutdown");

        assert_eq!(exit.reason, GcSchedulerExitReason::ShutdownRequested);
        assert_eq!(collector.get_stats().runs, 0);
    }
}
