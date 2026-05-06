use std::sync::Arc;

use andromeda_core::TransactionId;

use crate::active_snapshot_registry::ActiveSnapshotRegistry;
use crate::mvcc_status::{TransactionStatus, TransactionStatusTable};

use super::{VersionEligibility, VersionEligibilityChecker, VersionRecord};

fn make_status_table() -> Arc<TransactionStatusTable> {
    Arc::new(TransactionStatusTable::new())
}

fn make_snapshot_registry() -> Arc<ActiveSnapshotRegistry> {
    Arc::new(ActiveSnapshotRegistry::new())
}

fn next_tx_id() -> TransactionId {
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TX_ID: AtomicU64 = AtomicU64::new(1);
    TransactionId::new(NEXT_TX_ID.fetch_add(1, Ordering::Relaxed))
}

#[test]
fn test_version_eligibility_all_criteria_met() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, 2).unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    assert!(eligibility.is_creator_committed);
    assert!(eligibility.is_end_ts_invisible); // end_ts=100 < min_visible_ts (u64::MAX or default)
    assert!(eligibility.is_grace_period_met); // 10 >= 5 + 2
    assert!(eligibility.is_fully_eligible());
}

#[test]
fn test_version_eligibility_creator_not_committed() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, 2).unwrap();

    let tx_id = next_tx_id();
    // Do NOT mark as committed

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    assert!(!eligibility.is_creator_committed);
    assert!(!eligibility.is_fully_eligible());
}

#[test]
fn test_version_eligibility_grace_period_not_met() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, 5).unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
    let eligibility = checker.check_all_criteria(&version, 8).unwrap(); // 8 < 5 + 5

    assert!(eligibility.is_creator_committed);
    assert!(!eligibility.is_grace_period_met);
    assert!(!eligibility.is_fully_eligible());
}

#[test]
fn test_version_record_validation() {
    assert!(VersionRecord::new(0, next_tx_id(), 100, 5).is_err());

    let tx_id = next_tx_id();
    assert!(VersionRecord::new(1, tx_id, 100, 5).is_ok());
}

#[test]
fn test_eligibility_first_failed_criterion() {
    let elg1 = VersionEligibility {
        is_creator_committed: false,
        is_end_ts_invisible: true,
        is_grace_period_met: true,
    };
    assert_eq!(elg1.first_failed_criterion(), Some("creator not committed"));

    let elg2 = VersionEligibility {
        is_creator_committed: true,
        is_end_ts_invisible: false,
        is_grace_period_met: true,
    };
    assert_eq!(
        elg2.first_failed_criterion(),
        Some("end_ts still visible to snapshots")
    );

    let elg3 = VersionEligibility {
        is_creator_committed: true,
        is_end_ts_invisible: true,
        is_grace_period_met: false,
    };
    assert_eq!(
        elg3.first_failed_criterion(),
        Some("grace period not elapsed")
    );

    let elg_all_pass = VersionEligibility {
        is_creator_committed: true,
        is_end_ts_invisible: true,
        is_grace_period_met: true,
    };
    assert_eq!(elg_all_pass.first_failed_criterion(), None);
}

#[test]
fn test_checker_statistics_tracking() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, 2).unwrap();

    let tx_id_1 = next_tx_id();
    status_table.set_committed(tx_id_1).unwrap();

    let tx_id_2 = next_tx_id();
    // Do not commit tx_id_2

    let v1 = VersionRecord::new(1, tx_id_1, 100, 5).unwrap();
    let v2 = VersionRecord::new(2, tx_id_2, 100, 5).unwrap();

    checker.check_all_criteria(&v1, 10).unwrap();
    checker.check_all_criteria(&v2, 10).unwrap();

    let stats = checker.get_stats();
    assert_eq!(stats.versions_checked, 2);
    assert_eq!(stats.versions_eligible, 1);
    assert_eq!(stats.ineligible_creator_not_committed, 1);
}

#[test]
fn test_checker_reset_stats() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, 2).unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
    checker.check_all_criteria(&version, 10).unwrap();

    let stats = checker.get_stats();
    assert!(stats.versions_checked > 0);

    checker.reset_stats();
    let stats_after = checker.get_stats();
    assert_eq!(stats_after.versions_checked, 0);
}

#[test]
fn test_is_eligible_single_call() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, 2).unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
    let is_eligible = checker.is_eligible(&version, 10).unwrap();

    assert!(is_eligible);
}

#[test]
fn test_multiple_ineligible_reasons() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, 10).unwrap();

    let tx_id = next_tx_id();
    // Do NOT commit

    let version = VersionRecord::new(1, tx_id, 100, 0).unwrap();
    let eligibility = checker.check_all_criteria(&version, 5).unwrap(); // gc_epoch < grace_period

    assert!(!eligibility.is_creator_committed);
    assert!(!eligibility.is_grace_period_met);
    assert!(!eligibility.is_fully_eligible());

    let stats = checker.get_stats();
    assert_eq!(stats.versions_checked, 1);
    assert_eq!(stats.versions_eligible, 0);
    assert!(stats.ineligible_creator_not_committed > 0);
    assert!(stats.ineligible_grace_period > 0);
}

#[test]
fn test_grace_period_boundary_conditions() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let grace_period = 5;
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, grace_period)
            .unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

    let marked_at = 10;
    let version = VersionRecord::new(1, tx_id, 100, marked_at).unwrap();

    // Just below threshold
    let eligibility_below = checker.check_all_criteria(&version, marked_at + grace_period - 1);
    assert!(eligibility_below.is_ok());
    assert!(!eligibility_below.unwrap().is_grace_period_met);

    // Exactly at threshold
    let eligibility_at = checker.check_all_criteria(&version, marked_at + grace_period);
    assert!(eligibility_at.is_ok());
    assert!(eligibility_at.unwrap().is_grace_period_met);

    // Above threshold
    let eligibility_above = checker.check_all_criteria(&version, marked_at + grace_period + 1);
    assert!(eligibility_above.is_ok());
    assert!(eligibility_above.unwrap().is_grace_period_met);
}

#[test]
fn test_creator_status_defaults_to_inflight() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, 2).unwrap();

    let tx_id = next_tx_id();
    // Do NOT record this transaction

    let status = checker.creator_status(tx_id).unwrap();
    assert_eq!(status, TransactionStatus::InFlight);
}

#[test]
fn test_min_visible_timestamp_query() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker = VersionEligibilityChecker::new(status_table, snapshot_registry, 2).unwrap();

    let min_ts = checker.min_visible_timestamp().unwrap();
    assert_eq!(min_ts, u64::MAX);
}

#[test]
fn test_version_eligibility_check_all_criteria_independent() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, 2).unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 0).unwrap();

    // Test with gc_epoch = 0, so grace period definitely not met
    let eligibility = checker.check_all_criteria(&version, 0).unwrap();

    // Creator is committed
    // end_ts is invisible
    // Grace period NOT met
    assert!(eligibility.is_creator_committed);
    assert!(eligibility.is_end_ts_invisible);
    assert!(!eligibility.is_grace_period_met);
    assert!(!eligibility.is_fully_eligible());
}

#[test]
fn test_stats_monotonic_accumulation() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, 2).unwrap();

    let tx_id = next_tx_id();
    status_table.set_committed(tx_id).unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    for i in 0..10 {
        checker.check_all_criteria(&version, 10).unwrap();
        let stats = checker.get_stats();
        assert_eq!(stats.versions_checked, i as u64 + 1);
        assert_eq!(stats.versions_eligible, i as u64 + 1);
    }

    let final_stats = checker.get_stats();
    assert_eq!(final_stats.versions_checked, 10);
    assert_eq!(final_stats.versions_eligible, 10);
}

#[test]
fn test_grace_period_zero_rejected() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();

    let result = VersionEligibilityChecker::new(status_table, snapshot_registry, 0);
    assert!(result.is_err());
}
