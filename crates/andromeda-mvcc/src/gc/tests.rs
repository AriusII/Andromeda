use std::sync::Arc;

use andromeda_transaction_log::Lsn;
use andromeda_types::TransactionId;

use crate::active_snapshot_registry::{ActiveSnapshotRegistry, SnapshotHandle};
use crate::status::TransactionStatusTable;

use super::MvccGarbageCollector;

fn make_collector() -> MvccGarbageCollector {
    MvccGarbageCollector::new(
        Arc::new(ActiveSnapshotRegistry::new()),
        Arc::new(TransactionStatusTable::new()),
    )
}

#[test]
fn test_live_version_never_reclaimed() {
    let collector = make_collector();
    let tx_id = TransactionId::new(1);

    // Live version (end_ts = u64::MAX) should never be reclaimed
    assert!(!collector.is_version_reclaimable(tx_id, u64::MAX));
}

#[test]
fn test_in_flight_version_never_reclaimed() {
    let collector = make_collector();
    let tx_id = TransactionId::new(1);

    // InFlight creator: never reclaim (no durable evidence)
    assert!(!collector.is_version_reclaimable(tx_id, 100));
    assert!(!collector.is_version_reclaimable(tx_id, 500));
}

#[test]
fn test_rolled_back_version_always_reclaimed() {
    let status_table = TransactionStatusTable::new();
    let tx_id = TransactionId::new(1);

    // Record rollback
    status_table
        .record_rolled_back_after_durable_wal(tx_id, Lsn::new(1), Lsn::new(1))
        .expect("record rollback");

    let collector = MvccGarbageCollector::new(
        Arc::new(ActiveSnapshotRegistry::new()),
        Arc::new(status_table),
    );

    // Rolled-back versions are always reclaimable
    assert!(collector.is_version_reclaimable(tx_id, 1));
    assert!(collector.is_version_reclaimable(tx_id, 100));
    assert!(collector.is_version_reclaimable(tx_id, 1000));
}

#[test]
fn test_committed_version_older_than_min_visible() {
    let status_table = TransactionStatusTable::new();
    let registry = ActiveSnapshotRegistry::new();
    let tx_id = TransactionId::new(1);
    let other_tx = TransactionId::new(2);

    // Record creator as committed
    status_table
        .record_committed_after_durable_wal(tx_id, Lsn::new(1), Lsn::new(1))
        .expect("record commit");

    // Register a snapshot at ts=200 (min_visible will be 200)
    registry
        .register_snapshot(SnapshotHandle::new(200, other_tx).expect("snapshot"))
        .expect("register");

    let collector = MvccGarbageCollector::new(Arc::new(registry), Arc::new(status_table));

    // Version with end_ts < 200 should be reclaimable
    assert!(collector.is_version_reclaimable(tx_id, 199));
    assert!(collector.is_version_reclaimable(tx_id, 100));

    // Version with end_ts >= 200 should NOT be reclaimable
    assert!(!collector.is_version_reclaimable(tx_id, 200));
    assert!(!collector.is_version_reclaimable(tx_id, 250));
}

#[test]
fn test_committed_version_newer_than_min_visible() {
    let status_table = TransactionStatusTable::new();
    let registry = ActiveSnapshotRegistry::new();
    let tx_id = TransactionId::new(1);

    // Record creator as committed
    status_table
        .record_committed_after_durable_wal(tx_id, Lsn::new(1), Lsn::new(1))
        .expect("record commit");

    // No snapshots registered: min_visible = u64::MAX
    let collector = MvccGarbageCollector::new(Arc::new(registry), Arc::new(status_table));

    // No active snapshots means the minimum visible timestamp is u64::MAX.
    assert!(collector.is_version_reclaimable(tx_id, 1));
    assert!(collector.is_version_reclaimable(tx_id, 1_000_000));
}

#[test]
fn test_stats_tracking() {
    let collector = make_collector();

    assert_eq!(collector.get_stats().versions_scanned, 0);
    assert_eq!(collector.get_stats().versions_reclaimed, 0);
    assert_eq!(collector.get_stats().runs, 0);

    collector.record_versions_scanned(100);
    assert_eq!(collector.get_stats().versions_scanned, 100);

    collector.record_versions_reclaimed(25);
    assert_eq!(collector.get_stats().versions_reclaimed, 25);

    collector.run_gc().expect("gc run");
    assert_eq!(collector.get_stats().runs, 1);
    assert!(collector.get_stats().last_run_ms > 0);
}

#[test]
fn test_gc_summary_includes_min_visible() {
    let registry = ActiveSnapshotRegistry::new();
    let tx_id = TransactionId::new(1);

    registry
        .register_snapshot(SnapshotHandle::new(500, tx_id).expect("snapshot"))
        .expect("register");

    let collector =
        MvccGarbageCollector::new(Arc::new(registry), Arc::new(TransactionStatusTable::new()));

    let summary = collector.run_gc().expect("gc");
    assert_eq!(summary.min_visible_ts, 500);
}

#[test]
fn test_multiple_snapshots_min_visible() {
    let registry = ActiveSnapshotRegistry::new();
    let status_table = TransactionStatusTable::new();

    let tx1 = TransactionId::new(1);
    let tx2 = TransactionId::new(2);
    let tx3 = TransactionId::new(3);

    // Register three snapshots at different timestamps
    registry
        .register_snapshot(SnapshotHandle::new(500, tx1).expect("snapshot"))
        .expect("register");
    registry
        .register_snapshot(SnapshotHandle::new(200, tx2).expect("snapshot"))
        .expect("register");
    registry
        .register_snapshot(SnapshotHandle::new(800, tx3).expect("snapshot"))
        .expect("register");

    status_table
        .record_committed_after_durable_wal(tx1, Lsn::new(1), Lsn::new(1))
        .expect("record");

    let collector = MvccGarbageCollector::new(Arc::new(registry), Arc::new(status_table));

    // min_visible_ts should be the minimum: 200
    assert_eq!(collector.minimum_visible_timestamp(), 200);

    // Version with end_ts = 199 should be reclaimable
    assert!(collector.is_version_reclaimable(tx1, 199));

    // Version with end_ts = 200 should NOT be reclaimable
    assert!(!collector.is_version_reclaimable(tx1, 200));
}

#[test]
fn test_stats_accumulation() {
    let collector = make_collector();

    collector.record_versions_scanned(50);
    collector.record_versions_reclaimed(10);
    collector.record_versions_scanned(30);
    collector.record_versions_reclaimed(5);

    let stats = collector.get_stats();
    assert_eq!(stats.versions_scanned, 80);
    assert_eq!(stats.versions_reclaimed, 15);
}
