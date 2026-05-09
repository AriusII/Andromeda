//! End-to-end visibility gates for status transitions driven through
//! TransactionManager durable commit and rollback paths.

use andromeda_core::{CatalogVersion, TransactionId};
use andromeda_mvcc::{
    MvccIsolationPolicy, MvccRowHeader, Snapshot, TransactionStatus, TransactionStatusTable,
};
use andromeda_transaction::TransactionManager;
use andromeda_transaction_log::Lsn;

#[test]
fn mvcc_v0_visibility_requires_manager_durable_commit() {
    let manager = TransactionManager::new();
    let writer = manager.begin().unwrap();
    let reader = manager.begin().unwrap();
    let row = MvccRowHeader::open_version(10, writer, None).unwrap();

    let statuses = TransactionStatusTable::new();
    statuses
        .record(writer, manager.status(writer).unwrap().unwrap())
        .unwrap();
    statuses
        .record(reader, manager.status(reader).unwrap().unwrap())
        .unwrap();

    let reader_snapshot = Snapshot::with_context_validated(
        20,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        Some(reader),
        [writer],
        &statuses,
    )
    .unwrap();

    assert!(
        !row.visible_in_snapshot(&reader_snapshot, &statuses)
            .unwrap()
    );

    manager.request_commit(writer).unwrap();
    manager.commit_durable(writer, 4242).unwrap();
    statuses
        .record_committed_after_durable_wal(writer, Lsn::new(4242), Lsn::new(4242))
        .unwrap();
    assert_eq!(
        manager.status(writer).unwrap(),
        Some(TransactionStatus::Committed),
        "manager must mirror Committed only after durable LSN flush"
    );

    assert!(
        row.visible_in_snapshot(&reader_snapshot, &statuses)
            .unwrap()
    );
}

#[test]
fn mvcc_v0_manager_rollback_keeps_writes_invisible() {
    let manager = TransactionManager::new();
    let writer = manager.begin().unwrap();
    let reader = manager.begin().unwrap();
    let row = MvccRowHeader::open_version(10, writer, None).unwrap();

    manager.request_rollback(writer).unwrap();
    manager.rollback_durable(writer, 7777).unwrap();

    let statuses = TransactionStatusTable::new();
    statuses
        .record_rolled_back_after_durable_wal(writer, Lsn::new(7777), Lsn::new(7777))
        .unwrap();
    statuses
        .record(reader, manager.status(reader).unwrap().unwrap())
        .unwrap();

    let reader_snapshot = Snapshot::with_context_validated(
        100,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        Some(reader),
        Vec::<TransactionId>::new(),
        &statuses,
    )
    .unwrap();

    assert!(
        !row.visible_in_snapshot(&reader_snapshot, &statuses)
            .unwrap()
    );
}
