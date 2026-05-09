//! MVCC row and snapshot validation contracts.

use super::*;

/// TC-MV-0018
/// open_version requires non-zero begin_ts and creator_tx_id.
#[test]
fn mv_open_version_rejects_zero_begin_ts() {
    let err = MvccRowHeader::open_version(0, TransactionId::new(1), None).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-MV-0019
/// open_version rejects creator_tx_id == 0.
#[test]
fn mv_open_version_rejects_zero_creator_tx_id() {
    let err = MvccRowHeader::open_version(1, TransactionId::new(0), None).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-MV-0020
/// close_version requires end_ts > begin_ts.
#[test]
fn mv_close_version_rejects_end_ts_equal_to_begin_ts() {
    let row = MvccRowHeader::open_version(10, TransactionId::new(1), None).unwrap();
    let err = row.close_version(10, TransactionId::new(2)).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-MV-0021
/// close_version rejects end_ts < begin_ts.
#[test]
fn mv_close_version_rejects_end_ts_before_begin_ts() {
    let row = MvccRowHeader::open_version(10, TransactionId::new(1), None).unwrap();
    let err = row.close_version(9, TransactionId::new(2)).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-MV-0022
/// A closed row without a deleter_tx_id fails validation.
#[test]
fn mv_closed_row_without_deleter_tx_id_fails_validation() {
    let row = MvccRowHeader {
        begin_ts: 5,
        end_ts: Some(10),
        creator_tx_id: TransactionId::new(1),
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };
    let err = row.validate().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-MV-0023
/// An open row with a deleter_tx_id fails validation.
#[test]
fn mv_open_row_with_deleter_tx_id_fails_validation() {
    let row = MvccRowHeader {
        begin_ts: 5,
        end_ts: None,
        creator_tx_id: TransactionId::new(1),
        deleter_tx_id: Some(TransactionId::new(2)),
        previous_version_ptr: None,
        flags: 0,
    };
    let err = row.validate().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-MV-0028
/// Snapshot validates non-zero timestamp.
#[test]
fn mv_snapshot_rejects_zero_timestamp() {
    let err = Snapshot::with_context(
        0,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        None,
        [],
    )
    .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-MV-0029
/// Snapshot normalizes active_tx_ids (sort + dedup).
#[test]
fn mv_snapshot_normalizes_active_tx_ids() {
    let snapshot = Snapshot::with_context(
        10,
        CatalogVersion::new(1),
        MvccIsolationPolicy::RepeatableRead,
        Some(TransactionId::new(1)),
        [
            TransactionId::new(5),
            TransactionId::new(3),
            TransactionId::new(5),
        ],
    )
    .unwrap();
    assert_eq!(
        snapshot.active_tx_ids,
        vec![TransactionId::new(3), TransactionId::new(5)]
    );
}

/// TC-MV-0030
/// Snapshot with a terminal transaction in active_tx_ids is rejected.
#[test]
fn mv_snapshot_rejects_terminal_tx_in_active_list() {
    let status_table = TransactionStatusTable::new();
    let committed_tx = TransactionId::new(5);
    status_table
        .record_committed_after_durable_wal(
            committed_tx,
            durable_status_lsn(),
            durable_status_lsn(),
        )
        .unwrap();
    let owner = TransactionId::new(9);
    status_table
        .record(owner, TransactionStatus::InFlight)
        .unwrap();

    let snapshot = Snapshot::with_context(
        10,
        CatalogVersion::new(1),
        MvccIsolationPolicy::RepeatableRead,
        Some(owner),
        [committed_tx],
    )
    .unwrap();

    let err = snapshot
        .validate_against_statuses(&status_table)
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}
