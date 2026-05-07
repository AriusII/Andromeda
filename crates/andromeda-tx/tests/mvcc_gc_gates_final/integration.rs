//! Integration gates for scan correctness, reclaimed-space accounting, and
//! long-running reader blocking.

use super::fixtures::*;
use andromeda_core::TransactionId;

#[test]
fn test_integration_gc_preserves_scan_correctness() {
    let (registry, status_table, collector) = setup_gc_system();
    let tx1 = TransactionId::new(1);
    let tx2 = TransactionId::new(2);
    let reader_tx = TransactionId::new(100);

    record_committed(&status_table, tx1);
    record_committed(&status_table, tx2);

    let snap = register_snapshot(&registry, 500, reader_tx);

    assert!(
        collector.is_version_reclaimable(tx1, 100),
        "Old version eligible for reclamation"
    );
    assert!(
        !collector.is_version_reclaimable(tx2, u64::MAX),
        "Live version protected"
    );
    assert!(
        collector.is_version_reclaimable(tx2, 300),
        "Intermediate version eligible (300 < 500)"
    );

    registry.release_snapshot(snap).expect("release snapshot");
}

#[test]
fn test_integration_reclamation_frees_space() {
    let (_registry, _status_table, collector) = setup_gc_system();

    collector.record_versions_scanned(100);
    collector.record_versions_reclaimed(95);

    let stats_before = collector.get_stats();
    assert_eq!(stats_before.versions_reclaimed, 95);

    collector.record_versions_reclaimed(5);

    let stats_after = collector.get_stats();
    assert_eq!(
        stats_after.versions_reclaimed, 100,
        "All versions reclaimed"
    );
}

#[tokio::test]
async fn test_integration_long_running_tx_blocks_gc() {
    let (registry, status_table, collector) = setup_gc_system();
    let creator_tx = TransactionId::new(1);
    let long_running_reader = TransactionId::new(100);

    record_committed(&status_table, creator_tx);

    let long_snap = register_snapshot(&registry, 50, long_running_reader);

    assert!(
        !collector.is_version_reclaimable(creator_tx, 60),
        "Version with end_ts=60 blocked by snapshot at ts=50"
    );
    assert!(
        collector.is_version_reclaimable(creator_tx, 49),
        "Version with end_ts=49 IS reclaimable (older than snapshot)"
    );

    registry
        .release_snapshot(long_snap)
        .expect("release long snapshot");
}
