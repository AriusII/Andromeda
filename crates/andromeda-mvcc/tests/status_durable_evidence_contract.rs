use andromeda_error::AndromedaErrorKind;
use andromeda_mvcc::{
    MvccIsolationPolicy, MvccRowHeader, Snapshot, TransactionStatus, TransactionStatusTable,
};
use andromeda_transaction_log::Lsn;
use andromeda_types::{CatalogVersion, TransactionId};

fn tx(id: u64) -> TransactionId {
    TransactionId::new(id)
}

fn durable_lsn(value: u64) -> Lsn {
    Lsn::new(value)
}

fn snapshot(timestamp: u64, owner: TransactionId) -> Snapshot {
    Snapshot::with_context(
        timestamp,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        Some(owner),
        [],
    )
    .expect("valid read-committed snapshot")
}

#[test]
fn terminal_status_publication_requires_durable_wal_evidence() {
    let status_table = TransactionStatusTable::new();
    let committed = tx(8);
    let rolled_back = tx(9);

    let commit_error = status_table
        .record(committed, TransactionStatus::Committed)
        .expect_err("public committed status requires durable terminal evidence");
    assert_eq!(commit_error.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(status_table.status(committed), None);

    let set_committed_error = status_table
        .set_committed(committed)
        .expect_err("set_committed remains a rejected compatibility shim");
    assert_eq!(set_committed_error.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(status_table.status(committed), None);

    let rollback_error = status_table
        .record(rolled_back, TransactionStatus::RolledBack)
        .expect_err("public rolled-back status requires durable terminal evidence");
    assert_eq!(rollback_error.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(status_table.status(rolled_back), None);
}

#[test]
fn durable_terminal_status_publication_accepts_only_covered_lsn_evidence() {
    let status_table = TransactionStatusTable::new();
    let committed = tx(10);
    let rolled_back = tx(11);

    let short_commit_error = status_table
        .record_committed_after_durable_wal(committed, durable_lsn(20), durable_lsn(19))
        .expect_err("durable commit evidence must cover commit LSN");
    assert_eq!(short_commit_error.kind(), AndromedaErrorKind::Storage);
    assert_eq!(status_table.status(committed), None);

    status_table
        .record_committed_after_durable_wal(committed, durable_lsn(20), durable_lsn(20))
        .expect("covered commit evidence should publish committed status");
    assert_eq!(
        status_table.status(committed),
        Some(TransactionStatus::Committed)
    );

    let short_rollback_error = status_table
        .record_rolled_back_after_durable_wal(rolled_back, durable_lsn(30), durable_lsn(29))
        .expect_err("durable rollback evidence must cover rollback LSN");
    assert_eq!(short_rollback_error.kind(), AndromedaErrorKind::Storage);
    assert_eq!(status_table.status(rolled_back), None);

    status_table
        .record_rolled_back_after_durable_wal(rolled_back, durable_lsn(30), durable_lsn(31))
        .expect("covered rollback evidence should publish rolled-back status");
    assert_eq!(
        status_table.status(rolled_back),
        Some(TransactionStatus::RolledBack)
    );
}

#[test]
fn unknown_and_rolled_back_transactions_are_invisible_to_foreign_snapshots() {
    let status_table = TransactionStatusTable::new();
    let rolled_back = tx(12);
    let unknown = tx(13);
    let reader = tx(14);

    status_table
        .record_rolled_back_after_durable_wal(rolled_back, durable_lsn(40), durable_lsn(40))
        .expect("covered rollback evidence should publish rolled-back status");

    let snapshot = snapshot(100, reader);
    let rolled_back_row = MvccRowHeader::open_version(10, rolled_back, None).unwrap();
    let unknown_row = MvccRowHeader::open_version(10, unknown, None).unwrap();

    assert!(
        !rolled_back_row
            .visible_in_snapshot(&snapshot, &status_table)
            .unwrap()
    );
    assert!(
        !unknown_row
            .visible_in_snapshot(&snapshot, &status_table)
            .unwrap()
    );
}
