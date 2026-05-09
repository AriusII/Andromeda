use andromeda_error::AndromedaResult;
use andromeda_savepoint::{
    MAX_WRITE_SET_OPERATION_KIND_BYTES, MAX_WRITE_SET_RESOURCE_ID_BYTES, SavepointId,
    SavepointRollbackMarker, SavepointStack, TxWriteSet, WriteSetImage, WriteSetOperationKind,
    WriteSetResourceId,
};

fn text_resource(value: &str) -> AndromedaResult<WriteSetResourceId> {
    WriteSetResourceId::try_from_text(value)
}

fn image(value: &[u8]) -> AndromedaResult<WriteSetImage> {
    WriteSetImage::try_from_bytes(value.to_vec())
}

#[test]
fn savepoint_after_insert_uses_current_write_ordinal() -> AndromedaResult<()> {
    let mut write_set = TxWriteSet::new();
    let insert = write_set.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("table/users/row/1")?,
        None,
        Some(image(b"alice")?),
    )?;

    let mut savepoints = SavepointStack::new();
    let savepoint =
        savepoints.create_with_rollback_ordinal("after_insert", write_set.rollback_ordinal())?;
    let marker = write_set.savepoint_marker(savepoint.id)?;

    assert_eq!(insert.ordinal, 1);
    assert_eq!(savepoint.rollback_marker, marker);
    assert_eq!(savepoint.rollback_marker.rollback_ordinal, insert.ordinal);
    Ok(())
}

#[test]
fn rollback_to_savepoint_removes_only_later_insert() -> AndromedaResult<()> {
    let mut write_set = TxWriteSet::new();
    write_set.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("table/users/row/1")?,
        None,
        Some(image(b"alice")?),
    )?;

    let mut savepoints = SavepointStack::new();
    let savepoint =
        savepoints.create_with_rollback_ordinal("after_alice", write_set.rollback_ordinal())?;

    let later = write_set.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("table/users/row/2")?,
        None,
        Some(image(b"bob")?),
    )?;

    let rolled_back = write_set.rollback_to(savepoint.rollback_marker)?;

    assert_eq!(rolled_back.len(), 1);
    assert_eq!(rolled_back[0], later);
    assert_eq!(write_set.len(), 1);
    assert_eq!(
        write_set.entries()[0].resource_id,
        text_resource("table/users/row/1")?
    );
    Ok(())
}

#[test]
fn release_savepoint_keeps_all_writes() -> AndromedaResult<()> {
    let mut write_set = TxWriteSet::new();
    write_set.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("table/users/row/1")?,
        None,
        Some(image(b"alice")?),
    )?;

    let mut savepoints = SavepointStack::new();
    let savepoint =
        savepoints.create_with_rollback_ordinal("after_alice", write_set.rollback_ordinal())?;

    write_set.record_operation(
        WriteSetOperationKind::Update,
        text_resource("table/users/row/1")?,
        Some(image(b"alice")?),
        Some(image(b"alice-updated")?),
    )?;

    write_set.release(savepoint.rollback_marker)?;

    assert_eq!(write_set.len(), 2);
    assert_eq!(
        write_set
            .entries()
            .iter()
            .map(|entry| entry.operation_kind.clone())
            .collect::<Vec<_>>(),
        vec![WriteSetOperationKind::Insert, WriteSetOperationKind::Update]
    );
    Ok(())
}

#[test]
fn full_rollback_drains_all_writes() -> AndromedaResult<()> {
    let mut write_set = TxWriteSet::new();
    let first = write_set.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("table/users/row/1")?,
        None,
        Some(image(b"alice")?),
    )?;
    let second = write_set.record_operation(
        WriteSetOperationKind::Delete,
        text_resource("table/users/row/2")?,
        Some(image(b"bob")?),
        None,
    )?;

    let drained = write_set.full_rollback();

    assert_eq!(drained, vec![second, first]);
    assert!(write_set.is_empty());
    assert_eq!(write_set.rollback_ordinal(), 0);
    Ok(())
}

#[test]
fn rollback_preserves_earlier_operations_and_lock_placeholder() -> AndromedaResult<()> {
    let mut write_set = TxWriteSet::new();
    let lock_placeholder = write_set.record_operation(
        WriteSetOperationKind::LockEvidence,
        text_resource("lock/table/users")?,
        None,
        None,
    )?;
    let earlier_insert = write_set.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("table/users/row/1")?,
        None,
        Some(image(b"alice")?),
    )?;

    let mut savepoints = SavepointStack::new();
    let savepoint = savepoints
        .create_with_rollback_ordinal("after_lock_and_insert", write_set.rollback_ordinal())?;

    let later_update = write_set.record_operation(
        WriteSetOperationKind::Update,
        text_resource("table/users/row/1")?,
        Some(image(b"alice")?),
        Some(image(b"alice-updated")?),
    )?;
    let later_insert = write_set.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("table/users/row/2")?,
        None,
        Some(image(b"bob")?),
    )?;

    let rolled_back = write_set.rollback_to(savepoint.rollback_marker)?;

    assert_eq!(rolled_back, vec![later_insert, later_update]);
    assert_eq!(write_set.entries(), &[lock_placeholder, earlier_insert]);
    Ok(())
}

#[test]
fn rollback_returns_undo_entries_in_reverse_ordinal_and_preserves_later_lock_evidence()
-> AndromedaResult<()> {
    let mut write_set = TxWriteSet::new();
    let earlier_insert = write_set.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("table/users/row/1")?,
        None,
        Some(image(b"alice")?),
    )?;

    let mut savepoints = SavepointStack::new();
    let savepoint =
        savepoints.create_with_rollback_ordinal("after_alice", write_set.rollback_ordinal())?;

    let later_update = write_set.record_operation(
        WriteSetOperationKind::Update,
        text_resource("table/users/row/1")?,
        Some(image(b"alice")?),
        Some(image(b"alice-updated")?),
    )?;
    let lock_evidence = write_set.record_operation(
        WriteSetOperationKind::LockEvidence,
        text_resource("lock/table/users/row/2")?,
        None,
        None,
    )?;
    let later_delete = write_set.record_operation(
        WriteSetOperationKind::Delete,
        text_resource("table/users/row/2")?,
        Some(image(b"bob")?),
        None,
    )?;

    let rolled_back = write_set.rollback_to(savepoint.rollback_marker)?;

    assert_eq!(rolled_back, vec![later_delete, later_update]);
    assert_eq!(write_set.entries(), &[earlier_insert, lock_evidence]);
    Ok(())
}

#[test]
fn repeated_rollback_to_same_marker_is_idempotent_and_keeps_lock_evidence() -> AndromedaResult<()> {
    let mut write_set = TxWriteSet::new();
    let earlier_insert = write_set.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("table/users/row/1")?,
        None,
        Some(image(b"alice")?),
    )?;

    let mut savepoints = SavepointStack::new();
    let savepoint =
        savepoints.create_with_rollback_ordinal("after_alice", write_set.rollback_ordinal())?;

    let later_lock = write_set.record_operation(
        WriteSetOperationKind::LockEvidence,
        text_resource("lock/table/users/row/1")?,
        None,
        None,
    )?;
    let later_update = write_set.record_operation(
        WriteSetOperationKind::Update,
        text_resource("table/users/row/1")?,
        Some(image(b"alice")?),
        Some(image(b"alice-updated")?),
    )?;

    let first_rollback = write_set.rollback_to(savepoint.rollback_marker)?;
    let second_rollback = write_set.rollback_to(savepoint.rollback_marker)?;

    assert_eq!(first_rollback, vec![later_update]);
    assert!(second_rollback.is_empty());
    assert_eq!(write_set.entries(), &[earlier_insert, later_lock]);
    Ok(())
}

#[test]
fn full_rollback_drains_lock_evidence_at_transaction_boundary() -> AndromedaResult<()> {
    let mut write_set = TxWriteSet::new();
    let lock_evidence = write_set.record_operation(
        WriteSetOperationKind::LockEvidence,
        text_resource("lock/table/users")?,
        None,
        None,
    )?;
    let insert = write_set.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("table/users/row/1")?,
        None,
        Some(image(b"alice")?),
    )?;

    let drained = write_set.full_rollback();

    assert_eq!(drained, vec![insert, lock_evidence]);
    assert!(write_set.is_empty());
    Ok(())
}

#[test]
fn rollback_rejects_invalid_or_future_marker() -> AndromedaResult<()> {
    let mut write_set = TxWriteSet::new();
    write_set.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("table/users/row/1")?,
        None,
        Some(image(b"alice")?),
    )?;

    let invalid_marker = SavepointRollbackMarker {
        savepoint_id: SavepointId::new(0),
        rollback_ordinal: 0,
    };
    assert!(write_set.rollback_to(invalid_marker).is_err());

    let future_marker = SavepointRollbackMarker {
        savepoint_id: SavepointId::new(1),
        rollback_ordinal: write_set.rollback_ordinal() + 1,
    };
    assert!(write_set.rollback_to(future_marker).is_err());
    Ok(())
}

#[test]
fn record_operation_revalidates_directly_constructed_public_variants() {
    let mut write_set = TxWriteSet::new();

    assert!(
        write_set
            .record_operation(
                WriteSetOperationKind::Custom(String::new()),
                text_resource("table/users/row/1").unwrap(),
                None,
                None,
            )
            .is_err()
    );

    assert!(
        write_set
            .record_operation(
                WriteSetOperationKind::Custom("x".repeat(MAX_WRITE_SET_OPERATION_KIND_BYTES + 1)),
                text_resource("table/users/row/1").unwrap(),
                None,
                None,
            )
            .is_err()
    );

    assert!(
        write_set
            .record_operation(
                WriteSetOperationKind::Insert,
                WriteSetResourceId::Text(String::new()),
                None,
                None,
            )
            .is_err()
    );

    assert!(
        write_set
            .record_operation(
                WriteSetOperationKind::Insert,
                WriteSetResourceId::Bytes(vec![7; MAX_WRITE_SET_RESOURCE_ID_BYTES + 1]),
                None,
                None,
            )
            .is_err()
    );

    assert!(write_set.is_empty());
}
