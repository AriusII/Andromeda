//! Snapshot validation gates for terminal and in-flight owner membership.

use super::fixtures::durable_status_lsn;
use andromeda_core::{CatalogVersion, TransactionId};
use andromeda_tx::{MvccIsolationPolicy, Snapshot, TransactionStatus, TransactionStatusTable};

#[test]
fn snapshot_validation_rejects_terminal_owner() {
    let owner = TransactionId::new(1101);
    let statuses = TransactionStatusTable::new();
    statuses
        .record_committed_after_durable_wal(owner, durable_status_lsn(), durable_status_lsn())
        .unwrap();

    let err = Snapshot::with_context_validated(
        10,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        Some(owner),
        Vec::<TransactionId>::new(),
        &statuses,
    )
    .unwrap_err();
    assert!(err.message().contains("not in flight"));
}

#[test]
fn snapshot_validation_rejects_unregistered_owner() {
    let owner = TransactionId::new(1201);
    let statuses = TransactionStatusTable::new();

    let err = Snapshot::with_context_validated(
        10,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        Some(owner),
        Vec::<TransactionId>::new(),
        &statuses,
    )
    .unwrap_err();
    assert!(err.message().contains("not registered"));
}

#[test]
fn snapshot_validation_rejects_terminal_active_member() {
    let owner = TransactionId::new(1301);
    let stale = TransactionId::new(1302);
    let statuses = TransactionStatusTable::new();
    statuses.record(owner, TransactionStatus::InFlight).unwrap();
    statuses
        .record_rolled_back_after_durable_wal(stale, durable_status_lsn(), durable_status_lsn())
        .unwrap();

    let err = Snapshot::with_context_validated(
        10,
        CatalogVersion::new(1),
        MvccIsolationPolicy::RepeatableRead,
        Some(owner),
        [stale],
        &statuses,
    )
    .unwrap_err();
    assert!(err.message().contains("terminal transaction"));
}

#[test]
fn snapshot_validation_accepts_in_flight_owner_and_peers() {
    let owner = TransactionId::new(1401);
    let peer = TransactionId::new(1402);
    let statuses = TransactionStatusTable::new();
    statuses.record(owner, TransactionStatus::InFlight).unwrap();
    statuses.record(peer, TransactionStatus::InFlight).unwrap();

    let snapshot = Snapshot::with_context_validated(
        10,
        CatalogVersion::new(1),
        MvccIsolationPolicy::RepeatableRead,
        Some(owner),
        [peer],
        &statuses,
    )
    .unwrap();
    assert!(snapshot.is_transaction_active(peer));
    assert!(snapshot.is_current_transaction(owner));
}
