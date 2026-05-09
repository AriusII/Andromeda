//! Scheduler and atomic statistics gates for MVCC GC.

use super::fixtures::*;
use andromeda_core::TransactionId;
use andromeda_mvcc::GcSchedulerTask;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_scheduler_triggers_on_min_visible_ts_change() {
    let (registry, status_table, collector) = setup_gc_system();
    let tx1 = TransactionId::new(1);
    let reader_tx_1 = TransactionId::new(100);
    let reader_tx_2 = TransactionId::new(101);

    record_committed(&status_table, tx1);

    let snap1 = register_snapshot(&registry, 100, reader_tx_1);
    let snap2 = register_snapshot(&registry, 200, reader_tx_2);

    assert_eq!(
        collector.minimum_visible_timestamp(),
        100,
        "min_visible_ts initially 100"
    );

    registry.release_snapshot(snap1).expect("release snap1");

    assert_eq!(
        collector.minimum_visible_timestamp(),
        200,
        "min_visible_ts moved to 200 after snap1 release"
    );

    registry.release_snapshot(snap2).expect("release snap2");

    assert_eq!(
        collector.minimum_visible_timestamp(),
        u64::MAX,
        "min_visible_ts = u64::MAX when no snapshots"
    );
}

#[tokio::test]
async fn test_scheduler_background_thread_processes_on_trigger() {
    let (registry, status_table, collector) = setup_gc_system();
    let tx1 = TransactionId::new(1);
    let reader_tx = TransactionId::new(100);

    record_committed(&status_table, tx1);
    let snap = register_snapshot(&registry, 100, reader_tx);

    let scheduler = Arc::new(GcSchedulerTask::new(
        collector.clone(),
        Duration::from_millis(100),
    ));

    let scheduler_task = scheduler.clone();
    let handle = tokio::spawn(async move { scheduler_task.run_periodic_gc().await });

    tokio::time::sleep(Duration::from_millis(150)).await;

    registry.release_snapshot(snap).expect("release snapshot");

    tokio::time::sleep(Duration::from_millis(150)).await;
    handle.abort();

    let stats = collector.get_stats();
    assert!(
        stats.runs >= 1,
        "GC should have run at least once, got {} runs",
        stats.runs
    );
}

#[test]
fn test_scheduler_stats_updated_atomically() {
    let (_registry, _status_table, collector) = setup_gc_system();

    collector.record_versions_scanned(1000);
    collector.record_versions_reclaimed(950);

    let stats = collector.get_stats();
    assert_eq!(stats.versions_scanned, 1000, "Scanned count correct");
    assert_eq!(stats.versions_reclaimed, 950, "Reclaimed count correct");

    collector.record_versions_scanned(500);
    collector.record_versions_reclaimed(450);

    let stats = collector.get_stats();
    assert_eq!(stats.versions_scanned, 1500, "Cumulative scanned count");
    assert_eq!(stats.versions_reclaimed, 1400, "Cumulative reclaimed count");
}
