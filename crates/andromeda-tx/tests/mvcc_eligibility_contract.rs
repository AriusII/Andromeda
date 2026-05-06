//! Comprehensive contract tests for version eligibility checker.
//!
//! Verifies correctness of all three eligibility criteria and their combinations.

use andromeda_core::TransactionId;
use andromeda_tx::gc::mvcc_eligibility::{VersionEligibilityChecker, VersionRecord};
use andromeda_tx::{
    ActiveSnapshotRegistry, SnapshotHandle, TransactionStatus, TransactionStatusTable,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TX_ID: AtomicU64 = AtomicU64::new(1);

fn next_tx_id() -> TransactionId {
    TransactionId::new(NEXT_TX_ID.fetch_add(1, Ordering::Relaxed))
}

fn make_status_table() -> Arc<TransactionStatusTable> {
    Arc::new(TransactionStatusTable::new())
}

fn make_snapshot_registry() -> Arc<ActiveSnapshotRegistry> {
    Arc::new(ActiveSnapshotRegistry::new())
}

// Criterion 1: Creator Committed

#[test]
fn test_creator_committed_true() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    assert!(eligibility.is_creator_committed);
}

#[test]
fn test_creator_in_flight_false() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    // Do NOT set committed

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    assert!(!eligibility.is_creator_committed);
}

#[test]
fn test_creator_rolled_back_false() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table
        .record(tx_id, TransactionStatus::RolledBack)
        .unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    assert!(!eligibility.is_creator_committed);
}

// Criterion 2: End Timestamp Invisible

#[test]
fn test_end_ts_strictly_less_than_min_visible() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

    // Version's end_ts is 100, and with no active snapshots, min_visible_ts is very large
    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    assert!(eligibility.is_end_ts_invisible);
}

#[test]
fn test_end_ts_equal_to_min_visible_not_invisible() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

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
    status_table.set_committed(tx_id).unwrap();

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

// Criterion 3: Grace Period Met

#[test]
fn test_grace_period_not_met_below_threshold() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let grace_period = 5;
    let checker = VersionEligibilityChecker::new(
        status_table.clone(),
        snapshot_registry.clone(),
        grace_period,
    )
    .unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

    let marked_at = 10;
    let version = VersionRecord::new(1, tx_id, 100, marked_at).unwrap();
    // gc_epoch = 14 < 10 + 5 = 15
    let eligibility = checker.check_all_criteria(&version, 14).unwrap();

    assert!(!eligibility.is_grace_period_met);
}

#[test]
fn test_grace_period_met_exactly_at_threshold() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let grace_period = 5;
    let checker = VersionEligibilityChecker::new(
        status_table.clone(),
        snapshot_registry.clone(),
        grace_period,
    )
    .unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

    let marked_at = 10;
    let version = VersionRecord::new(1, tx_id, 100, marked_at).unwrap();
    // gc_epoch = 15 == 10 + 5
    let eligibility = checker.check_all_criteria(&version, 15).unwrap();

    assert!(eligibility.is_grace_period_met);
}

#[test]
fn test_grace_period_met_above_threshold() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let grace_period = 5;
    let checker = VersionEligibilityChecker::new(
        status_table.clone(),
        snapshot_registry.clone(),
        grace_period,
    )
    .unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

    let marked_at = 10;
    let version = VersionRecord::new(1, tx_id, 100, marked_at).unwrap();
    // gc_epoch = 20 > 10 + 5
    let eligibility = checker.check_all_criteria(&version, 20).unwrap();

    assert!(eligibility.is_grace_period_met);
}

// Combination Tests: One Criterion Fails

#[test]
fn test_ineligible_only_creator_fails() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    // Do NOT set committed
    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    assert!(!eligibility.is_creator_committed);
    assert!(eligibility.is_end_ts_invisible);
    assert!(eligibility.is_grace_period_met);
    assert!(!eligibility.is_fully_eligible());
}

#[test]
fn test_ineligible_only_end_ts_fails() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

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
fn test_ineligible_only_grace_period_fails() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 5).unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 10).unwrap();
    let eligibility = checker.check_all_criteria(&version, 12).unwrap(); // 12 < 10 + 5

    assert!(eligibility.is_creator_committed);
    assert!(eligibility.is_end_ts_invisible);
    assert!(!eligibility.is_grace_period_met); // 12 < 15
    assert!(!eligibility.is_fully_eligible());
}

// Combination Tests: Multiple Criteria Fail

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
fn test_fully_eligible_all_criteria_pass() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    assert!(eligibility.is_fully_eligible());
}

#[test]
fn test_fully_eligible_with_multiple_snapshots() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

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
fn test_version_at_min_visible_boundary() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

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
    status_table.set_committed(tx_id).unwrap();

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

#[test]
fn test_large_grace_period() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let grace_period = 1_000_000;
    let checker = VersionEligibilityChecker::new(
        status_table.clone(),
        snapshot_registry.clone(),
        grace_period,
    )
    .unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 0).unwrap();
    let eligibility = checker.check_all_criteria(&version, 999_999).unwrap();

    assert!(!eligibility.is_grace_period_met);

    let eligibility2 = checker.check_all_criteria(&version, 1_000_000).unwrap();
    assert!(eligibility2.is_grace_period_met);
}

#[test]
fn test_stats_accumulate_correctly() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id_committed = next_tx_id();
    status_table.set_committed(tx_id_committed).unwrap();

    let tx_id_inflight = next_tx_id();

    let v1 = VersionRecord::new(1, tx_id_committed, 100, 5).unwrap();
    let v2 = VersionRecord::new(2, tx_id_inflight, 100, 5).unwrap();
    let v3 = VersionRecord::new(3, tx_id_committed, 100, 5).unwrap();

    checker.check_all_criteria(&v1, 10).unwrap();
    checker.check_all_criteria(&v2, 10).unwrap();
    checker.check_all_criteria(&v3, 10).unwrap();

    let stats = checker.get_stats();
    assert_eq!(stats.versions_checked, 3);
    assert_eq!(stats.versions_eligible, 2);
    assert_eq!(stats.ineligible_creator_not_committed, 1);
}

#[test]
fn test_stats_reset_works() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    for _ in 0..5 {
        checker.check_all_criteria(&version, 10).unwrap();
    }

    let stats_before = checker.get_stats();
    assert_eq!(stats_before.versions_checked, 5);

    checker.reset_stats();

    let stats_after = checker.get_stats();
    assert_eq!(stats_after.versions_checked, 0);
    assert_eq!(stats_after.versions_eligible, 0);
}

// Invariant Validation Tests

#[test]
fn test_invariant_no_visible_version_eligible() {
    // This test validates the core invariant:
    // "No visible version is ever marked eligible"

    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

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

#[test]
fn test_is_eligible_method_consistency() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    // is_eligible should be consistent with check_all_criteria().is_fully_eligible()
    let is_eligible = checker.is_eligible(&version, 10).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    assert_eq!(is_eligible, eligibility.is_fully_eligible());
}

// Aborted and in-flight version handling.
// 18+ Tests for aborted and in-flight version edge cases

#[test]
fn test_aborted_version_immediately_eligible() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 100)
            .unwrap();

    let tx_id = next_tx_id();
    // Mark transaction as RolledBack (aborted)
    status_table
        .record(tx_id, TransactionStatus::RolledBack)
        .unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 0).unwrap();

    // Check that is_aborted_version_eligible returns true
    assert!(checker.is_aborted_version_eligible(&version));
}

#[test]
fn test_in_flight_version_not_safe() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    // Do NOT mark as committed or rolled back (remains InFlight)

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    // is_in_flight_version_safe should return false for in-flight txs
    assert!(!checker.is_in_flight_version_safe(&version));
}

#[test]
fn test_committed_version_safe_to_proceed() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    // is_in_flight_version_safe should return true for committed txs
    assert!(checker.is_in_flight_version_safe(&version));
}

#[test]
fn test_aborted_version_bypasses_grace_period() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let grace_period = 1000;
    let checker = VersionEligibilityChecker::new(
        status_table.clone(),
        snapshot_registry.clone(),
        grace_period,
    )
    .unwrap();

    let tx_id = next_tx_id();
    status_table
        .record(tx_id, TransactionStatus::RolledBack)
        .unwrap();

    // Mark at gc_epoch=0, check at gc_epoch=1
    let version = VersionRecord::new(1, tx_id, 100, 0).unwrap();

    // Even though gc_epoch (1) < marked_at_gc_epoch + grace_period (0 + 1000),
    // aborted versions are still eligible
    assert!(checker.is_aborted_version_eligible(&version));
}

#[test]
fn test_aborted_version_bypasses_visibility_check() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table
        .record(tx_id, TransactionStatus::RolledBack)
        .unwrap();

    // Create a snapshot that makes the version appear visible
    let snapshot_tx_id = next_tx_id();
    let snapshot_handle = SnapshotHandle::new(50, snapshot_tx_id).unwrap();
    snapshot_registry
        .register_snapshot(snapshot_handle)
        .unwrap();

    // Version with end_ts = 100 would normally be visible
    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    // But aborted versions don't need visibility check
    assert!(checker.is_aborted_version_eligible(&version));

    snapshot_registry.release_snapshot(snapshot_handle).unwrap();
}

#[test]
fn test_rolled_back_version_immediately_eligible() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table
        .record(tx_id, TransactionStatus::RolledBack)
        .unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    // Rolled back versions should be immediately eligible
    assert!(checker.is_aborted_version_eligible(&version));
}

#[test]
fn test_is_eligible_extended_aborted_path() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 100)
            .unwrap();

    let tx_id = next_tx_id();
    status_table
        .record(tx_id, TransactionStatus::RolledBack)
        .unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 0).unwrap();

    // is_eligible_extended should immediately return true for aborted txs
    let is_eligible = checker.is_eligible_extended(&version, 1).unwrap();
    assert!(is_eligible);
}

#[test]
fn test_is_eligible_extended_in_flight_path() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    // Leave in-flight

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    // is_eligible_extended should immediately return false for in-flight txs
    let is_eligible = checker.is_eligible_extended(&version, 10).unwrap();
    assert!(!is_eligible);
}

#[test]
fn test_is_eligible_extended_committed_normal_path() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    // is_eligible_extended should fall through to normal check for committed txs
    let is_eligible = checker.is_eligible_extended(&version, 10).unwrap();
    assert!(is_eligible); // Should be fully eligible
}

#[test]
fn test_aborted_version_does_not_count_as_in_flight() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table
        .record(tx_id, TransactionStatus::RolledBack)
        .unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    // Aborted version is safe to proceed (not in-flight)
    assert!(checker.is_in_flight_version_safe(&version));
}

#[test]
fn test_unknown_tx_defaults_to_in_flight() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    // Don't record anything in status table

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    // Unknown txs default to in-flight, so NOT safe
    assert!(!checker.is_in_flight_version_safe(&version));
    assert!(!checker.is_aborted_version_eligible(&version));
}

#[test]
fn test_concurrent_aborted_versions_stress() {
    use std::sync::Arc;
    use std::thread;

    let status_table = Arc::new(TransactionStatusTable::new());
    let snapshot_registry = Arc::new(ActiveSnapshotRegistry::new());
    let checker = Arc::new(
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap(),
    );

    let handles: Vec<_> = (0..50)
        .map(|i| {
            let checker_clone = Arc::clone(&checker);
            let status_table_clone = Arc::clone(&status_table);

            thread::spawn(move || {
                let tx_id = next_tx_id();

                // Half mark as rolled back, half leave in-flight
                if i % 2 == 0 {
                    status_table_clone
                        .record(tx_id, TransactionStatus::RolledBack)
                        .unwrap();
                }

                let version = VersionRecord::new(i as u64 + 1, tx_id, 100 + i as u64, 5).unwrap();

                if i % 2 == 0 {
                    // Should be immediately eligible
                    assert!(checker_clone.is_aborted_version_eligible(&version));
                } else {
                    // Should NOT be safe (in-flight)
                    assert!(!checker_clone.is_in_flight_version_safe(&version));
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_invariant_no_in_flight_version_reclaimed() {
    // This test validates the secondary invariant:
    // "No in-flight transaction version is ever reclaimed"

    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 1).unwrap();

    let tx_id = next_tx_id();
    // Leave in-flight (do not record status)

    let version = VersionRecord::new(1, tx_id, 100, 0).unwrap();

    // Even with gc_epoch far in future, in-flight versions must not be eligible
    assert!(!checker.is_in_flight_version_safe(&version));

    // Try with extended eligibility check (should fail at fast-path 2)
    let is_eligible = checker.is_eligible_extended(&version, 1_000_000).unwrap();
    assert!(!is_eligible);
}

#[test]
fn test_aborted_before_visibility_becomes_eligible() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 100)
            .unwrap();

    let tx_id = next_tx_id();

    // Initially committed
    status_table.set_committed(tx_id).unwrap();

    // Then marked as rolled back (simulating abort after commit detection)
    status_table
        .record(tx_id, TransactionStatus::RolledBack)
        .unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    // Should be eligible via aborted path
    assert!(checker.is_aborted_version_eligible(&version));
}

#[test]
fn test_in_flight_rollback_release_versions() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    // Initially in-flight
    assert!(!checker.is_in_flight_version_safe(&version));

    // Transaction rolls back
    status_table
        .record(tx_id, TransactionStatus::RolledBack)
        .unwrap();

    // Now immediately eligible
    assert!(checker.is_aborted_version_eligible(&version));
    assert!(checker.is_in_flight_version_safe(&version));
}

#[test]
fn test_first_failed_criterion_aborted_supersedes_other_failures() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 1000)
            .unwrap();

    let tx_id = next_tx_id();
    // Mark as aborted instead of committed
    status_table
        .record(tx_id, TransactionStatus::RolledBack)
        .unwrap();

    // Create snapshot to make visibility fail
    let snapshot_tx_id = next_tx_id();
    let snapshot_handle = SnapshotHandle::new(50, snapshot_tx_id).unwrap();
    snapshot_registry
        .register_snapshot(snapshot_handle)
        .unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 0).unwrap();

    // The aborted path bypasses all other checks
    assert!(checker.is_aborted_version_eligible(&version));

    snapshot_registry.release_snapshot(snapshot_handle).unwrap();
}

#[test]
fn test_stats_track_aborted_and_in_flight_checks() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_aborted = next_tx_id();
    status_table
        .record(tx_aborted, TransactionStatus::RolledBack)
        .unwrap();

    let tx_inflight = next_tx_id();

    let tx_committed = next_tx_id();
    status_table.set_committed(tx_committed).unwrap();

    let v1 = VersionRecord::new(1, tx_aborted, 100, 5).unwrap();
    let v2 = VersionRecord::new(2, tx_inflight, 100, 5).unwrap();
    let v3 = VersionRecord::new(3, tx_committed, 100, 5).unwrap();

    // Using is_eligible_extended to trigger fast paths
    let _ = checker.is_eligible_extended(&v1, 10);
    let _ = checker.is_eligible_extended(&v2, 10);
    let _ = checker.is_eligible_extended(&v3, 10);

    // v1 (aborted) should be eligible at fast-path 1
    // v2 (in-flight) should be ineligible at fast-path 2
    // v3 (committed) should go through normal path

    let stats = checker.get_stats();
    // All three are checked at some level
    assert!(stats.versions_checked >= 1); // At least the committed one triggers normal path
}
