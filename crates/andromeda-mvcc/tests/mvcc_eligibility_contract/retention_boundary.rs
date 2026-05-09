//! Retention-frontier boundary coverage. These cases prove the strict end_ts < min_visible_ts rule.

use super::*;

#[test]
fn test_end_ts_strictly_less_than_min_visible() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table
        .record_committed_after_durable_wal(
            tx_id,
            andromeda_transaction_log::Lsn::new(1),
            andromeda_transaction_log::Lsn::new(1),
        )
        .unwrap();

    // Version's end_ts is 100, and with no active snapshots, min_visible_ts is very large
    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    assert!(eligibility.is_end_ts_invisible);
}
#[test]
fn test_ineligible_only_end_ts_fails() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table
        .record_committed_after_durable_wal(
            tx_id,
            andromeda_transaction_log::Lsn::new(1),
            andromeda_transaction_log::Lsn::new(1),
        )
        .unwrap();

    // Register a snapshot at timestamp 50 to set min_visible_ts = 50
    let snapshot_tx_id = next_tx_id();
    let snapshot_handle = SnapshotHandle::new(50, snapshot_tx_id).unwrap();
    snapshot_registry
        .register_snapshot(snapshot_handle)
        .unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    assert!(eligibility.is_creator_committed);
    assert!(!eligibility.is_end_ts_invisible); // 100 >= 50
    assert!(eligibility.is_grace_period_met);
    assert!(!eligibility.is_fully_eligible());

    snapshot_registry.release_snapshot(snapshot_handle).unwrap();
}
#[test]
fn test_ineligible_creator_and_end_ts_fail() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    // Do NOT set committed

    let snapshot_tx_id = next_tx_id();
    let snapshot_handle = SnapshotHandle::new(100, snapshot_tx_id).unwrap();
    snapshot_registry
        .register_snapshot(snapshot_handle)
        .unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    assert!(!eligibility.is_creator_committed);
    assert!(!eligibility.is_end_ts_invisible); // 100 >= 100
    assert!(eligibility.is_grace_period_met);
    assert!(!eligibility.is_fully_eligible());

    snapshot_registry.release_snapshot(snapshot_handle).unwrap();
}
#[test]
fn test_ineligible_all_three_criteria_fail() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 10)
            .unwrap();

    let tx_id = next_tx_id();
    // Do NOT set committed

    let snapshot_tx_id = next_tx_id();
    let snapshot_handle = SnapshotHandle::new(50, snapshot_tx_id).unwrap();
    snapshot_registry
        .register_snapshot(snapshot_handle)
        .unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 0).unwrap();
    // gc_epoch = 5 < 0 + 10
    let eligibility = checker.check_all_criteria(&version, 5).unwrap();

    assert!(!eligibility.is_creator_committed);
    assert!(!eligibility.is_end_ts_invisible); // 100 >= 50
    assert!(!eligibility.is_grace_period_met); // 5 < 10
    assert!(!eligibility.is_fully_eligible());

    snapshot_registry.release_snapshot(snapshot_handle).unwrap();
}
#[test]
fn test_version_at_min_visible_boundary() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table
        .record_committed_after_durable_wal(
            tx_id,
            andromeda_transaction_log::Lsn::new(1),
            andromeda_transaction_log::Lsn::new(1),
        )
        .unwrap();

    let snapshot_tx_id = next_tx_id();
    let snapshot_handle = SnapshotHandle::new(100, snapshot_tx_id).unwrap();
    snapshot_registry
        .register_snapshot(snapshot_handle)
        .unwrap();

    // Version's end_ts is exactly at the boundary (100)
    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    // end_ts == min_visible_ts, so NOT invisible (strict <)
    assert!(!eligibility.is_end_ts_invisible);
    assert!(!eligibility.is_fully_eligible());

    snapshot_registry.release_snapshot(snapshot_handle).unwrap();
}
#[test]
fn test_version_just_below_min_visible_boundary() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table
        .record_committed_after_durable_wal(
            tx_id,
            andromeda_transaction_log::Lsn::new(1),
            andromeda_transaction_log::Lsn::new(1),
        )
        .unwrap();

    let snapshot_tx_id = next_tx_id();
    let snapshot_handle = SnapshotHandle::new(100, snapshot_tx_id).unwrap();
    snapshot_registry
        .register_snapshot(snapshot_handle)
        .unwrap();

    // Version's end_ts is just below the boundary (99)
    let version = VersionRecord::new(1, tx_id, 99, 5).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    // end_ts < min_visible_ts, so IS invisible
    assert!(eligibility.is_end_ts_invisible);
    assert!(eligibility.is_fully_eligible());

    snapshot_registry.release_snapshot(snapshot_handle).unwrap();
}
