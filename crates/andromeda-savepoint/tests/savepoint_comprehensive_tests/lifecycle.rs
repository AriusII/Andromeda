use super::*;

#[test]
fn create_allocates_monotonic_ids_and_preserves_stack_order() -> AndromedaResult<()> {
    let mut stack = SavepointStack::new();

    let first = stack.create("first")?;
    let second = stack.create("second")?;
    let third = stack.create("third")?;

    assert_eq!(first.id.get(), 1);
    assert_eq!(second.id.get(), 2);
    assert_eq!(third.id.get(), 3);
    assert_eq!(names(&stack), vec!["first", "second", "third"]);
    assert_eq!(stack.depth(), 3);
    Ok(())
}

#[test]
fn create_with_rollback_ordinal_binds_marker_to_external_write_position() -> AndromedaResult<()> {
    let mut write_set = TxWriteSet::new();
    write_set.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("table/accounts/row/1")?,
        None,
        Some(image(b"before")?),
    )?;

    let mut stack = SavepointStack::new();
    let savepoint =
        stack.create_with_rollback_ordinal("after_insert", write_set.rollback_ordinal())?;

    assert_eq!(savepoint.id.get(), 1);
    assert_eq!(savepoint.rollback_marker.rollback_ordinal, 1);
    assert_eq!(
        write_set.savepoint_marker(savepoint.id)?,
        savepoint.rollback_marker
    );
    Ok(())
}

#[test]
fn rollback_to_keeps_target_and_discards_descendants_in_stack_order() -> AndromedaResult<()> {
    let mut stack = SavepointStack::new();
    stack.create("root")?;
    let target = stack.create("mid")?;
    let child = stack.create("child")?;
    let leaf = stack.create("leaf")?;

    let rollback = stack.rollback_to("mid")?;

    assert_eq!(rollback.target, target);
    assert_eq!(rollback.discarded_descendants, vec![child, leaf]);
    assert_eq!(names(&stack), vec!["root", "mid"]);
    Ok(())
}

#[test]
fn repeated_rollback_to_same_target_is_idempotent_after_descendants_are_gone() -> AndromedaResult<()>
{
    let mut stack = SavepointStack::new();
    stack.create("checkpoint")?;
    stack.create("work_a")?;
    stack.create("work_b")?;

    let first = stack.rollback_to("checkpoint")?;
    let second = stack.rollback_to("checkpoint")?;

    assert_eq!(first.discarded_descendants.len(), 2);
    assert!(second.discarded_descendants.is_empty());
    assert_eq!(names(&stack), vec!["checkpoint"]);
    Ok(())
}

#[test]
fn release_removes_target_and_all_nested_savepoints() -> AndromedaResult<()> {
    let mut stack = SavepointStack::new();
    stack.create("root")?;
    let child = stack.create("child")?;
    let grandchild = stack.create("grandchild")?;

    let release = stack.release("child")?;

    assert_eq!(release.released, vec![child, grandchild]);
    assert_eq!(names(&stack), vec!["root"]);
    Ok(())
}

#[test]
fn clear_resets_active_names_but_not_the_id_allocator() -> AndromedaResult<()> {
    let mut stack = SavepointStack::new();
    stack.create("first")?;
    stack.create("second")?;

    stack.clear();
    let after_clear = stack.create("first")?;

    assert_eq!(names(&stack), vec!["first"]);
    assert_eq!(after_clear.id.get(), 3);
    Ok(())
}

#[test]
fn name_can_be_reused_after_rollback_discards_that_savepoint() -> AndromedaResult<()> {
    let mut stack = SavepointStack::new();
    stack.create("base")?;
    stack.create("work")?;

    stack.rollback_to("base")?;
    let reused = stack.create("work")?;

    assert_eq!(reused.name, "work");
    assert_eq!(names(&stack), vec!["base", "work"]);
    Ok(())
}

#[test]
fn savepoint_id_conversion_validates_zero_without_changing_new_roundtrip() -> AndromedaResult<()> {
    assert_eq!(SavepointId::new(0).get(), 0);
    assert!(!SavepointId::new(0).is_valid());

    let id = SavepointId::try_from(42)?;
    assert_eq!(u64::from(id), 42);
    assert_transaction_error(SavepointId::try_from(0));
    Ok(())
}
