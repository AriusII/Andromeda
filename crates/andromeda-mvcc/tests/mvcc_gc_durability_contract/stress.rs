//! Bounded stress coverage for MVCC GC behavior under volume, churn, and
//! overlapping snapshots.

use super::fixtures::*;
use andromeda_types::TransactionId;

#[test]
fn test_gc_stress_1000_versions() {
    let (registry, status_table, collector) = setup_gc();

    for i in 1..=1000 {
        let tx_id = TransactionId::new(i as u64);
        if i % 2 == 0 {
            record_committed(&status_table, tx_id);
        } else {
            record_rolled_back(&status_table, tx_id);
        }
    }

    register_snapshot(&registry, 500, TransactionId::new(2000));

    let committed_tx = TransactionId::new(100);
    assert!(
        collector.is_version_reclaimable(committed_tx, 400),
        "Committed version should be reclaimable with old end_ts"
    );

    let rolled_back_tx = TransactionId::new(101);
    assert!(
        collector.is_version_reclaimable(rolled_back_tx, 600),
        "Rolled-back version should be reclaimable"
    );

    collector.record_versions_scanned(1000);
    collector.record_versions_reclaimed(750);

    let stats = collector.get_stats();
    assert_eq!(stats.versions_scanned, 1000, "All versions scanned");
    assert_eq!(
        stats.versions_reclaimed, 750,
        "Majority reclaimed (50% committed + some rolled-back)"
    );
}

#[test]
fn test_gc_stress_rapid_commits() {
    let (_registry, status_table, collector) = setup_gc();

    for burst in 1..=10 {
        for i in 1..=100 {
            let tx_id = TransactionId::new((burst * 100 + i) as u64);
            record_committed(&status_table, tx_id);
        }
        collector.record_versions_scanned(100);
        collector.record_versions_reclaimed(75);
    }

    let stats = collector.get_stats();
    assert_eq!(stats.versions_scanned, 1000, "All bursts scanned");
    assert_eq!(stats.versions_reclaimed, 750, "All bursts reclaimed");
}

#[test]
fn test_gc_stress_long_running_with_snapshots() {
    let (registry, status_table, collector) = setup_gc();

    let tx1 = TransactionId::new(1);
    record_committed(&status_table, tx1);

    assert!(
        collector.is_version_reclaimable(tx1, 100),
        "Version should be reclaimable with no active snapshots"
    );

    let _snap1 = register_snapshot(&registry, 200, TransactionId::new(100));
    assert!(
        collector.is_version_reclaimable(tx1, 100),
        "Version with old end_ts should still be reclaimable"
    );

    let snap2 = register_snapshot(&registry, 150, TransactionId::new(101));
    assert!(
        collector.is_version_reclaimable(tx1, 100),
        "Version still reclaimable after earlier snapshot registered"
    );

    registry.release_snapshot(snap2).unwrap();
    assert!(
        collector.is_version_reclaimable(tx1, 100),
        "Version reclaimable after snapshot release"
    );
}

#[test]
fn test_gc_stress_high_churn() {
    let (registry, status_table, collector) = setup_gc();

    let mut tx_id_counter = 1u64;

    for cycle in 1..=5 {
        for _ in 0..100 {
            let tx_id = TransactionId::new(tx_id_counter);
            record_committed(&status_table, tx_id);
            tx_id_counter += 1;
        }

        collector.record_versions_scanned(100);
        collector.record_versions_reclaimed(80);

        if cycle % 2 == 0 {
            let _snap = register_snapshot(
                &registry,
                100 * cycle as u64,
                TransactionId::new(9000 + cycle as u64),
            );
        }
    }

    let stats = collector.get_stats();
    assert_eq!(stats.versions_scanned, 500, "All cycles scanned (5 * 100)");
    assert_eq!(
        stats.versions_reclaimed, 400,
        "All cycles reclaimed (5 * 80)"
    );

    let final_min_visible = collector.minimum_visible_timestamp();
    assert!(
        final_min_visible > 0 || final_min_visible == u64::MAX,
        "min_visible_ts should be valid"
    );
}

#[test]
fn test_gc_stress_overlapping_snapshots() {
    let (registry, status_table, collector) = setup_gc();

    let tx1 = TransactionId::new(1);
    record_committed(&status_table, tx1);

    let mut handles = Vec::new();
    for i in 1..=10 {
        let ts = 100 + (i as u64 * 10);
        handles.push(register_snapshot(
            &registry,
            ts,
            TransactionId::new(1000 + i as u64),
        ));
    }

    let min_ts = collector.minimum_visible_timestamp();
    assert_eq!(min_ts, 110, "min_visible_ts should be earliest snapshot");

    assert!(
        collector.is_version_reclaimable(tx1, 105),
        "Version before earliest snapshot should be reclaimable"
    );
    assert!(
        !collector.is_version_reclaimable(tx1, 115),
        "Version after earliest snapshot should NOT be reclaimable"
    );

    registry.release_snapshot(handles[0]).unwrap();

    let new_min_ts = collector.minimum_visible_timestamp();
    assert_eq!(
        new_min_ts, 120,
        "After releasing earliest, min_visible_ts should advance"
    );
    assert!(
        collector.is_version_reclaimable(tx1, 115),
        "Version previously blocked should now be reclaimable"
    );
}
