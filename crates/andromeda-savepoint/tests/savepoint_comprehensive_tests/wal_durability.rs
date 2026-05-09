use super::*;

#[test]
fn savepoint_stack_operations_are_in_memory_metadata_only() -> AndromedaResult<()> {
    let mut stack = SavepointStack::new();

    let savepoint = stack.create("no_wal")?;
    let rollback = stack.rollback_to("no_wal")?;
    let release = stack.release("no_wal")?;

    assert_eq!(rollback.target, savepoint);
    assert!(rollback.discarded_descendants.is_empty());
    assert_eq!(release.released, vec![savepoint]);
    assert!(stack.is_empty());
    Ok(())
}

#[test]
fn rollback_evidence_contains_stack_facts_but_no_durable_outcome() -> AndromedaResult<()> {
    let mut stack = SavepointStack::new();
    let root = stack.create_with_rollback_ordinal("root", 0)?;
    let child = stack.create_with_rollback_ordinal("child", 5)?;
    let leaf = stack.create_with_rollback_ordinal("leaf", 8)?;

    let evidence = stack.rollback_to("root")?;

    assert_eq!(evidence.target, root);
    assert_eq!(evidence.discarded_descendants, vec![child, leaf]);
    assert_eq!(evidence.target.rollback_marker.rollback_ordinal, 0);
    Ok(())
}

#[test]
fn release_evidence_contains_released_savepoints_without_touching_write_set() -> AndromedaResult<()>
{
    let mut stack = SavepointStack::new();
    stack.create("before_write")?;
    stack.create("after_write")?;

    let mut write_set = TxWriteSet::new();
    let write = write_set.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("table/orders/row/1")?,
        None,
        Some(image(b"order")?),
    )?;

    let release = stack.release("before_write")?;

    assert_eq!(release.released.len(), 2);
    assert_eq!(write_set.entries(), &[write]);
    Ok(())
}

#[test]
fn rollback_to_savepoint_drains_only_entries_after_marker() -> AndromedaResult<()> {
    let mut write_set = TxWriteSet::new();
    let retained = write_set.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("table/orders/row/1")?,
        None,
        Some(image(b"one")?),
    )?;
    let marker = SavepointRollbackMarker::new(SavepointId::new(1), write_set.rollback_ordinal())?;
    let undone = write_set.record_operation(
        WriteSetOperationKind::Update,
        text_resource("table/orders/row/1")?,
        Some(image(b"one")?),
        Some(image(b"two")?),
    )?;

    let undo_entries = write_set.rollback_to(marker)?;

    assert_eq!(undo_entries, vec![undone]);
    assert_eq!(write_set.entries(), &[retained]);
    Ok(())
}

#[test]
fn lock_evidence_survives_partial_savepoint_rollback_until_full_rollback() -> AndromedaResult<()> {
    let mut write_set = TxWriteSet::new();
    let retained = write_set.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("table/orders/row/1")?,
        None,
        Some(image(b"one")?),
    )?;
    let marker = SavepointRollbackMarker::new(SavepointId::new(1), write_set.rollback_ordinal())?;
    let lock_evidence = write_set.record_operation(
        WriteSetOperationKind::LockEvidence,
        text_resource("lock/table/orders/row/1")?,
        None,
        None,
    )?;
    let undone = write_set.record_operation(
        WriteSetOperationKind::Delete,
        text_resource("table/orders/row/2")?,
        Some(image(b"two")?),
        None,
    )?;

    assert_eq!(write_set.rollback_to(marker)?, vec![undone]);
    assert_eq!(
        write_set.entries(),
        &[retained.clone(), lock_evidence.clone()]
    );

    assert_eq!(write_set.full_rollback(), vec![lock_evidence, retained]);
    assert!(write_set.is_empty());
    Ok(())
}
