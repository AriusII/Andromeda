//! Long-reader and active-snapshot coverage. These cases keep reader visibility ahead of GC reclamation decisions.

use super::*;

#[test]
fn test_end_ts_equal_to_min_visible_not_invisible() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table
        .record_committed_after_durable_wal(
            tx_id,
            andromeda_tx::Lsn::new(1),
            andromeda_tx::Lsn::new(1),
        )
        .unwrap();

    // Register a snapshot at timestamp 100 to set min_visible_ts = 100
    let snapshot_tx_id = next_tx_id();
    let snapshot_handle = SnapshotHandle::new(100, snapshot_tx_id).unwrap();
    snapshot_registry
        .register_snapshot(snapshot_handle)
        .unwrap();

    // Version's end_ts = 100, equal to min_visible_ts, so NOT invisible
    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    assert!(!eligibility.is_end_ts_invisible);

    snapshot_registry.release_snapshot(snapshot_handle).unwrap();
}
#[test]
fn test_end_ts_greater_than_min_visible_not_invisible() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table
        .record_committed_after_durable_wal(
            tx_id,
            andromeda_tx::Lsn::new(1),
            andromeda_tx::Lsn::new(1),
        )
        .unwrap();

    // Register a snapshot at timestamp 50 to set min_visible_ts = 50
    let snapshot_tx_id = next_tx_id();
    let snapshot_handle = SnapshotHandle::new(50, snapshot_tx_id).unwrap();
    snapshot_registry
        .register_snapshot(snapshot_handle)
        .unwrap();

    // Version's end_ts = 100, greater than min_visible_ts = 50, so NOT invisible
    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    assert!(!eligibility.is_end_ts_invisible);

    snapshot_registry.release_snapshot(snapshot_handle).unwrap();
}
#[test]
fn test_fully_eligible_with_multiple_snapshots() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table
        .record_committed_after_durable_wal(
            tx_id,
            andromeda_tx::Lsn::new(1),
            andromeda_tx::Lsn::new(1),
        )
        .unwrap();

    // Register multiple snapshots at older timestamps
    let snapshot_handles: Vec<_> = (0..5)
        .map(|i| {
            let stx_id = next_tx_id();
            let handle = SnapshotHandle::new(1000 + i as u64, stx_id).unwrap();
            snapshot_registry.register_snapshot(handle).unwrap();
            handle
        })
        .collect();

    // Version's end_ts = 50 is still less than min_visible_ts = 1000
    let version = VersionRecord::new(1, tx_id, 50, 5).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    assert!(eligibility.is_fully_eligible());

    for handle in snapshot_handles {
        snapshot_registry.release_snapshot(handle).unwrap();
    }
}
#[test]
fn test_invariant_no_visible_version_eligible() {
    // This test validates the core invariant:
    // "No visible version is ever marked eligible"

    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table
        .record_committed_after_durable_wal(
            tx_id,
            andromeda_tx::Lsn::new(1),
            andromeda_tx::Lsn::new(1),
        )
        .unwrap();

    // Create a snapshot at timestamp 50
    let snapshot_tx_id = next_tx_id();
    let snapshot_handle = SnapshotHandle::new(50, snapshot_tx_id).unwrap();
    snapshot_registry
        .register_snapshot(snapshot_handle)
        .unwrap();

    // A version with end_ts = 51 is visible to the snapshot (creator committed and ts >= 51)
    let version = VersionRecord::new(1, tx_id, 51, 5).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    // By the invariant, this version should NOT be marked eligible
    assert!(!eligibility.is_fully_eligible());
    // Specifically, it should fail the end_ts invisibility check
    assert!(!eligibility.is_end_ts_invisible);

    snapshot_registry.release_snapshot(snapshot_handle).unwrap();
}
