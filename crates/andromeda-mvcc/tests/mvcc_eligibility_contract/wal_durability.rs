//! Durable-WAL status coverage. These cases only create committed or rolled-back status through durable WAL evidence helpers.

use super::*;

#[test]
fn test_creator_committed_true() {
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

    assert!(eligibility.is_creator_committed);
}
#[test]
fn test_creator_rolled_back_false() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 2).unwrap();

    let tx_id = next_tx_id();
    status_table
        .record_rolled_back_after_durable_wal(
            tx_id,
            andromeda_transaction_log::Lsn::new(1),
            andromeda_transaction_log::Lsn::new(1),
        )
        .unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
    let eligibility = checker.check_all_criteria(&version, 10).unwrap();

    assert!(!eligibility.is_creator_committed);
}
#[test]
fn test_committed_version_safe_to_proceed() {
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

    // is_in_flight_version_safe should return true for committed txs
    assert!(checker.is_in_flight_version_safe(&version));
}
#[test]
fn test_is_eligible_extended_committed_normal_path() {
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

    // is_eligible_extended should fall through to normal check for committed txs
    let is_eligible = checker.is_eligible_extended(&version, 10).unwrap();
    assert!(is_eligible); // Should be fully eligible
}
#[test]
fn test_aborted_before_visibility_becomes_eligible() {
    let status_table = make_status_table();
    let snapshot_registry = make_snapshot_registry();
    let checker =
        VersionEligibilityChecker::new(status_table.clone(), snapshot_registry.clone(), 100)
            .unwrap();

    let tx_id = next_tx_id();

    // Mark as rolled back with durable WAL evidence before any commit visibility.
    status_table
        .record_rolled_back_after_durable_wal(
            tx_id,
            andromeda_transaction_log::Lsn::new(1),
            andromeda_transaction_log::Lsn::new(1),
        )
        .unwrap();

    let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

    // Should be eligible via aborted path
    assert!(checker.is_aborted_version_eligible(&version));
}
