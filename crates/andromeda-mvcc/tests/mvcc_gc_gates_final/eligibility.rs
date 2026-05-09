//! Eligibility gate coverage for MVCC GC.

use super::fixtures::*;
use andromeda_types::TransactionId;

#[test]
fn test_eligibility_criteria_creator_committed_and_end_ts_invisible() {
    let (registry, status_table, collector) = setup_gc_system();
    let tx1 = TransactionId::new(1);
    let tx2 = TransactionId::new(2);

    register_snapshot(&registry, 300, tx2);

    assert!(
        !collector.is_version_reclaimable(tx1, 200),
        "Version NOT reclaimable when creator not committed"
    );

    record_committed(&status_table, tx1);

    assert!(
        collector.is_version_reclaimable(tx1, 200),
        "Version reclaimable when creator committed AND end_ts < min_visible_ts"
    );
}

#[test]
fn test_eligibility_preserves_visible_versions() {
    let (registry, status_table, collector) = setup_gc_system();
    let tx1 = TransactionId::new(1);
    let reader_tx = TransactionId::new(100);

    record_committed(&status_table, tx1);
    register_snapshot(&registry, 200, reader_tx);

    assert!(
        !collector.is_version_reclaimable(tx1, 200),
        "Boundary version end_ts=200 not reclaimable when min_visible_ts=200"
    );
    assert!(
        collector.is_version_reclaimable(tx1, 199),
        "Version with end_ts=199 IS reclaimable when min_visible_ts=200"
    );
    assert!(
        !collector.is_version_reclaimable(tx1, 201),
        "Future version end_ts=201 not reclaimable"
    );
}

#[test]
fn test_eligibility_snapshot_release_enables_reclamation() {
    let (registry, status_table, collector) = setup_gc_system();
    let tx1 = TransactionId::new(1);
    let reader_tx = TransactionId::new(100);

    record_committed(&status_table, tx1);

    let snapshot_handle = register_snapshot(&registry, 300, reader_tx);

    assert!(
        !collector.is_version_reclaimable(tx1, 300),
        "Before snapshot release: version not reclaimable"
    );

    registry
        .release_snapshot(snapshot_handle)
        .expect("release snapshot");

    assert!(
        collector.is_version_reclaimable(tx1, 300),
        "After snapshot release: version IS reclaimable"
    );
}

#[test]
fn test_eligibility_live_versions_never_reclaimed() {
    let (registry, status_table, collector) = setup_gc_system();
    let tx1 = TransactionId::new(1);
    let tx2 = TransactionId::new(2);

    record_committed(&status_table, tx1);
    register_snapshot(&registry, 500, tx2);

    assert!(
        !collector.is_version_reclaimable(tx1, u64::MAX),
        "Live version never reclaimable"
    );
}

#[test]
fn test_eligibility_rolled_back_always_reclaimable() {
    let (_registry, status_table, collector) = setup_gc_system();
    let tx1 = TransactionId::new(1);

    record_rolled_back(&status_table, tx1);

    assert!(
        collector.is_version_reclaimable(tx1, 0),
        "Rolled-back version with end_ts=0 reclaimable"
    );
    assert!(
        collector.is_version_reclaimable(tx1, 1000),
        "Rolled-back version with end_ts=1000 reclaimable"
    );
}
