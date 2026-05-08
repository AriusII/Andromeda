use andromeda_core::{AndromedaResult, CatalogVersion, TransactionId};
use andromeda_tx::{
    LockAcquireStatus, LockHolder, LockManager, LockMode, LockResource, Lsn, MvccIsolationPolicy,
    MvccRowHeader, SavepointStack, Snapshot, TransactionStatus, TransactionStatusTable, TxWriteSet,
    WriteSetImage, WriteSetOperationKind, WriteSetResourceId,
};

fn text_resource(value: &str) -> AndromedaResult<WriteSetResourceId> {
    WriteSetResourceId::try_from_text(value)
}

fn image(value: &[u8]) -> AndromedaResult<WriteSetImage> {
    WriteSetImage::try_from_bytes(value.to_vec())
}

fn retained_text_resources(write_set: &TxWriteSet) -> Vec<String> {
    write_set
        .entries()
        .iter()
        .filter_map(|entry| match &entry.resource_id {
            WriteSetResourceId::Text(value) => Some(value.clone()),
            WriteSetResourceId::Bytes(_) => None,
        })
        .collect()
}

#[test]
fn rollback_to_savepoint_preserves_outer_transaction_lock_ownership() -> AndromedaResult<()> {
    let lock_manager = LockManager::new();
    let owner_tx = TransactionId::new(101);
    let waiter_tx = TransactionId::new(102);
    let row_resource = LockResource::row(10, 20, 30)?;

    assert_eq!(
        lock_manager.acquire(owner_tx, row_resource, LockMode::Exclusive)?,
        LockAcquireStatus::Granted
    );

    let mut write_set = TxWriteSet::new();
    let lock_evidence = write_set.record_operation(
        WriteSetOperationKind::LockEvidence,
        text_resource("lock/schema/10/table/20/row/30")?,
        None,
        None,
    )?;

    let mut savepoints = SavepointStack::new();
    let savepoint =
        savepoints.create_with_rollback_ordinal("after_lock", write_set.rollback_ordinal())?;

    let later_update = write_set.record_operation(
        WriteSetOperationKind::Update,
        text_resource("table/20/row/30")?,
        Some(image(b"before")?),
        Some(image(b"after")?),
    )?;

    let rollback = savepoints.rollback_to("after_lock")?;
    let undo_entries = write_set.rollback_to(rollback.target.rollback_marker)?;

    assert_eq!(rollback.target.id, savepoint.id);
    assert!(rollback.discarded_descendants.is_empty());
    assert_eq!(undo_entries, vec![later_update]);
    assert_eq!(write_set.entries(), &[lock_evidence]);

    let holder = LockHolder::new(owner_tx, LockMode::Exclusive)?;
    assert_eq!(
        lock_manager.entry(row_resource)?.map(|entry| entry.holders),
        Some(vec![holder])
    );

    let wait_status = lock_manager.acquire(waiter_tx, row_resource, LockMode::Shared)?;
    assert!(
        matches!(wait_status, LockAcquireStatus::Waiting { .. }),
        "savepoint rollback must not release the owner transaction's lock"
    );
    if let LockAcquireStatus::Waiting { blockers, .. } = wait_status {
        assert_eq!(blockers, vec![holder]);
    }

    Ok(())
}

#[test]
fn rollback_to_savepoint_removes_later_writes_from_local_view() -> AndromedaResult<()> {
    let writer_tx = TransactionId::new(201);
    let statuses = TransactionStatusTable::new();
    statuses.record_in_flight(writer_tx)?;

    let same_transaction_snapshot = Snapshot::with_context(
        20,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        Some(writer_tx),
        [writer_tx],
    )?;
    let external_snapshot = Snapshot::with_context(
        20,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        None,
        [],
    )?;

    let before_savepoint_row = MvccRowHeader::open_version(10, writer_tx, None)?;
    let after_savepoint_row = MvccRowHeader::open_version(11, writer_tx, None)?;

    assert!(before_savepoint_row.visible_in_snapshot(&same_transaction_snapshot, &statuses)?);
    assert!(after_savepoint_row.visible_in_snapshot(&same_transaction_snapshot, &statuses)?);
    assert!(!before_savepoint_row.visible_in_snapshot(&external_snapshot, &statuses)?);
    assert!(!after_savepoint_row.visible_in_snapshot(&external_snapshot, &statuses)?);

    let mut write_set = TxWriteSet::new();
    let earlier_insert = write_set.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("table/20/row/1")?,
        None,
        Some(image(b"row-1")?),
    )?;

    let mut savepoints = SavepointStack::new();
    let savepoint =
        savepoints.create_with_rollback_ordinal("after_row_1", write_set.rollback_ordinal())?;

    let later_insert = write_set.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("table/20/row/2")?,
        None,
        Some(image(b"row-2")?),
    )?;

    assert_eq!(
        retained_text_resources(&write_set),
        vec!["table/20/row/1".to_string(), "table/20/row/2".to_string()]
    );

    let rollback = savepoints.rollback_to("after_row_1")?;
    let undo_entries = write_set.rollback_to(rollback.target.rollback_marker)?;

    assert_eq!(rollback.target, savepoint);
    assert_eq!(undo_entries, vec![later_insert]);
    assert_eq!(write_set.entries(), &[earlier_insert]);
    assert_eq!(
        retained_text_resources(&write_set),
        vec!["table/20/row/1".to_string()]
    );

    Ok(())
}

#[test]
fn commit_visibility_requires_durable_lsn_coverage() -> AndromedaResult<()> {
    let writer_tx = TransactionId::new(301);
    let statuses = TransactionStatusTable::new();
    statuses.record_in_flight(writer_tx)?;

    let row = MvccRowHeader::open_version(10, writer_tx, None)?;
    let external_snapshot = Snapshot::with_context(
        20,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        None,
        [],
    )?;

    assert!(!row.visible_in_snapshot(&external_snapshot, &statuses)?);
    assert!(statuses.set_committed(writer_tx).is_err());
    assert_eq!(
        statuses.status(writer_tx),
        Some(TransactionStatus::InFlight)
    );
    assert!(!row.visible_in_snapshot(&external_snapshot, &statuses)?);

    assert!(
        statuses
            .record_committed_after_durable_wal(writer_tx, Lsn::new(42), Lsn::new(41))
            .is_err()
    );
    assert_eq!(
        statuses.status(writer_tx),
        Some(TransactionStatus::InFlight)
    );
    assert!(!row.visible_in_snapshot(&external_snapshot, &statuses)?);

    statuses.record_committed_after_durable_wal(writer_tx, Lsn::new(42), Lsn::new(42))?;

    assert_eq!(
        statuses.status(writer_tx),
        Some(TransactionStatus::Committed)
    );
    assert!(row.visible_in_snapshot(&external_snapshot, &statuses)?);
    Ok(())
}
