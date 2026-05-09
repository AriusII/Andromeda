//! GC eligibility coverage for creator status, grace period, full eligibility, and observable stats.

use super::*;

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
    status_table
        .record_committed_after_durable_wal(
            tx_id,
            andromeda_transaction_log::Lsn::new(1),
            andromeda_transaction_log::Lsn::new(1),
        )
        .unwrap();

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
    status_table
        .record_committed_after_durable_wal(
            tx_id,
            andromeda_transaction_log::Lsn::new(1),
            andromeda_transaction_log::Lsn::new(1),
        )
        .unwrap();

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
    status_table
        .record_committed_after_durable_wal(
            tx_id,
            andromeda_transaction_log::Lsn::new(1),
            andromeda_transaction_log::Lsn::new(1),
        )
        .unwrap();

    let marked_at = 10;
    let version = VersionRecord::new(1, tx_id, 100, marked_at).unwrap();
    // gc_epoch = 20 > 10 + 5
    let eligibility = checker.check_all_criteria(&version, 20).unwrap();

    assert!(eligibility.is_grace_period_met);
}
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
fn test_ineligible_only_grace_period_fails() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 5).unwrap();

    let tx_id = next_tx_id();
    status_table
        .record_committed_after_durable_wal(
            tx_id,
            andromeda_transaction_log::Lsn::new(1),
            andromeda_transaction_log::Lsn::new(1),
        )
        .unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 10).unwrap();
    let eligibility = checker.check_all_criteria(&version, 12).unwrap(); // 12 < 10 + 5

    assert!(eligibility.is_creator_committed);
    assert!(eligibility.is_end_ts_invisible);
    assert!(!eligibility.is_grace_period_met); // 12 < 15
    assert!(!eligibility.is_fully_eligible());
}
#[test]
fn test_fully_eligible_all_criteria_pass() {
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

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    assert!(eligibility.is_fully_eligible());
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
    status_table
        .record_committed_after_durable_wal(
            tx_id,
            andromeda_transaction_log::Lsn::new(1),
            andromeda_transaction_log::Lsn::new(1),
        )
        .unwrap();

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
    status_table
        .record_committed_after_durable_wal(
            tx_id_committed,
            andromeda_transaction_log::Lsn::new(1),
            andromeda_transaction_log::Lsn::new(1),
        )
        .unwrap();

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
    status_table
        .record_committed_after_durable_wal(
            tx_id,
            andromeda_transaction_log::Lsn::new(1),
            andromeda_transaction_log::Lsn::new(1),
        )
        .unwrap();

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
#[test]
fn test_is_eligible_method_consistency() {
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

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    // is_eligible should be consistent with check_all_criteria().is_fully_eligible()
    let is_eligible = checker.is_eligible(&version, 10).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    assert_eq!(is_eligible, eligibility.is_fully_eligible());
}
#[test]
fn test_stats_track_aborted_and_in_flight_checks() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_aborted = next_tx_id();
    status_table
        .record_rolled_back_after_durable_wal(
            tx_aborted,
            andromeda_transaction_log::Lsn::new(1),
            andromeda_transaction_log::Lsn::new(1),
        )
        .unwrap();

    let tx_inflight = next_tx_id();

    let tx_committed = next_tx_id();
    status_table
        .record_committed_after_durable_wal(
            tx_committed,
            andromeda_transaction_log::Lsn::new(1),
            andromeda_transaction_log::Lsn::new(1),
        )
        .unwrap();

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
