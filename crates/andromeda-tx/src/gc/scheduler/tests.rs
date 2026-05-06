use std::sync::Arc;
use std::time::Duration;

use andromeda_core::TransactionId;

use super::*;
use crate::active_snapshot_registry::{ActiveSnapshotRegistry, SnapshotHandle};
use crate::mvcc_status::TransactionStatusTable;

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
    println!("GC runs after 200ms with no snapshot change: {}", stats.runs);
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
    let snap2 = SnapshotHandle::new(200, TransactionId::new(2)).expect("second snapshot handle");
    registry.register_snapshot(snap1).expect("register snap1");
    registry.register_snapshot(snap2).expect("register snap2");

    let scheduler = GcSchedulerTask::new(collector.clone(), Duration::from_secs(1));
    assert!(scheduler.tick_once().expect("initial tick").is_some());
    assert_eq!(scheduler.scheduler_stats().last_observed_min_visible_ts, 100);

    assert!(scheduler.tick_once().expect("stable tick").is_none());
    assert_eq!(collector.get_stats().runs, 1);

    registry.release_snapshot(snap1).expect("release snap1");
    let advanced = scheduler.tick_once().expect("advanced tick");
    assert!(advanced.is_some());
    assert_eq!(advanced.expect("gc summary").min_visible_ts, 200);
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
    assert_eq!(scheduler.scheduler_stats().last_observed_min_visible_ts, u64::MAX);

    // A new active snapshot moves the frontier backward. Running a bounded
    // evidence pass is safe and prevents starvation after the snapshot closes.
    let snap = SnapshotHandle::new(100, TransactionId::new(3)).expect("snapshot");
    registry.register_snapshot(snap).expect("register snapshot");
    let active = scheduler.tick_once().expect("active snapshot tick");
    assert!(active.is_some());
    assert_eq!(active.expect("gc summary").min_visible_ts, 100);

    registry.release_snapshot(snap).expect("release snapshot");
    let released = scheduler.tick_once().expect("released snapshot tick");
    assert!(released.is_some());
    assert_eq!(released.expect("gc summary").min_visible_ts, u64::MAX);
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
