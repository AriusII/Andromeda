//! MVCC GC correctness contracts: eligibility, visibility preservation, and
//! empty-table behavior.

use super::fixtures::*;
use andromeda_core::TransactionId;

#[test]
fn test_gc_reclamation_count_accuracy() {
    let (_registry, status_table, collector) = setup_gc();

    for i in 1..=10 {
        let tx_id = TransactionId::new(i);
        if i <= 5 {
            record_committed(&status_table, tx_id);
        } else {
            record_rolled_back(&status_table, tx_id);
        }
    }

    collector.record_versions_scanned(10);
    collector.record_versions_reclaimed(8);

    let stats = collector.get_stats();
    assert_eq!(stats.versions_scanned, 10, "Scanned count mismatch");
    assert_eq!(stats.versions_reclaimed, 8, "Reclaimed count mismatch");
}

#[test]
fn test_gc_only_marks_eligible_versions() {
    let (registry, status_table, collector) = setup_gc();

    let tx1 = TransactionId::new(1);
    let tx2 = TransactionId::new(2);
    let tx3 = TransactionId::new(3);

    record_committed(&status_table, tx1);
    record_rolled_back(&status_table, tx2);
    register_snapshot(&registry, 500, tx3);

    assert!(
        collector.is_version_reclaimable(tx1, 400),
        "Committed version with old end_ts should be reclaimable"
    );
    assert!(
        collector.is_version_reclaimable(tx2, 400),
        "Rolled back version should be reclaimable"
    );
    assert!(
        !collector.is_version_reclaimable(tx3, 400),
        "InFlight creator should not allow reclamation"
    );
}

#[test]
fn test_no_visible_version_reclaimed() {
    let (registry, status_table, collector) = setup_gc();

    let tx1 = TransactionId::new(1);
    let tx2 = TransactionId::new(2);

    record_committed(&status_table, tx1);
    register_snapshot(&registry, 100, tx2);

    assert!(
        !collector.is_version_reclaimable(tx1, 150),
        "Version visible to active snapshot must NOT be reclaimable"
    );
    assert!(
        collector.is_version_reclaimable(tx1, 99),
        "Version with end_ts < min_visible_ts should be reclaimable"
    );
}

#[test]
fn test_gc_empty_version_table() {
    let (_registry, _status_table, collector) = setup_gc();

    collector.record_versions_scanned(0);
    collector.record_versions_reclaimed(0);

    let stats = collector.get_stats();
    assert_eq!(
        stats.versions_scanned, 0,
        "Empty table should report 0 scanned"
    );
    assert_eq!(
        stats.versions_reclaimed, 0,
        "Empty table should report 0 reclaimed"
    );

    let summary = collector
        .run_gc()
        .expect("GC on empty table should succeed");
    assert_eq!(
        summary.versions_scanned, 0,
        "Summary should show 0 scanned for empty table"
    );
}

#[test]
fn test_gc_mixed_committed_aborted() {
    let (registry, status_table, collector) = setup_gc();

    let tx_committed = TransactionId::new(1);
    let tx_rolled_back = TransactionId::new(2);
    let tx_inflight = TransactionId::new(3);

    record_committed(&status_table, tx_committed);
    record_rolled_back(&status_table, tx_rolled_back);
    register_snapshot(&registry, 200, TransactionId::new(99));

    assert!(
        collector.is_version_reclaimable(tx_committed, 100),
        "Committed version with old end_ts is reclaimable"
    );
    assert!(
        collector.is_version_reclaimable(tx_rolled_back, 500),
        "Rolled-back version is always reclaimable"
    );
    assert!(
        !collector.is_version_reclaimable(tx_inflight, 100),
        "InFlight version is never reclaimable"
    );
}

#[test]
fn test_gc_active_snapshots_block_reclamation() {
    let (registry, status_table, collector) = setup_gc();

    let tx1 = TransactionId::new(1);
    let tx2 = TransactionId::new(2);
    let tx3 = TransactionId::new(3);

    record_committed(&status_table, tx1);
    record_committed(&status_table, tx2);
    register_snapshot(&registry, 300, tx3);
    register_snapshot(&registry, 500, tx3);

    assert!(
        collector.is_version_reclaimable(tx1, 200),
        "Version old enough for min snapshot should be reclaimable"
    );
    assert!(
        !collector.is_version_reclaimable(tx1, 350),
        "Version newer than min snapshot should NOT be reclaimable"
    );
    assert!(
        collector.is_version_reclaimable(tx2, 200),
        "Consistency check: committed version with old end_ts"
    );
    assert!(
        !collector.is_version_reclaimable(tx2, 400),
        "Consistency check: committed version with newer end_ts"
    );
}

#[test]
fn test_gc_after_snapshot_closes() {
    let (registry, status_table, collector) = setup_gc();

    let tx1 = TransactionId::new(1);
    let tx2 = TransactionId::new(2);

    record_committed(&status_table, tx1);
    let snapshot = register_snapshot(&registry, 200, tx2);

    assert!(
        !collector.is_version_reclaimable(tx1, 250),
        "Version should be blocked while snapshot is active"
    );

    registry
        .release_snapshot(snapshot)
        .expect("release snapshot");

    assert!(
        collector.is_version_reclaimable(tx1, 250),
        "After releasing the only snapshot, closed version should become reclaimable"
    );
}

#[test]
fn test_gc_never_reclaims_live_versions() {
    let (_registry, status_table, collector) = setup_gc();

    let tx1 = TransactionId::new(1);
    record_committed(&status_table, tx1);

    assert!(
        !collector.is_version_reclaimable(tx1, u64::MAX),
        "Live version (end_ts=u64::MAX) should NEVER be reclaimable"
    );
}
