use super::*;

#[test]
fn create_rejects_empty_whitespace_and_duplicate_names_without_mutation() -> AndromedaResult<()> {
    let mut stack = SavepointStack::new();

    assert_transaction_error(stack.create(""));
    assert_transaction_error(stack.create("   "));
    assert!(stack.is_empty());

    let alpha = stack.create("alpha")?;
    assert_transaction_error(stack.create("alpha"));

    assert_eq!(stack.active(), &[alpha]);
    Ok(())
}

#[test]
fn rollback_to_missing_name_leaves_stack_unchanged() -> AndromedaResult<()> {
    let mut stack = SavepointStack::new();
    stack.create("outer")?;
    stack.create("inner")?;
    let before = stack.active().to_vec();

    assert_transaction_error(stack.rollback_to("missing"));

    assert_eq!(stack.active(), before.as_slice());
    Ok(())
}

#[test]
fn release_missing_name_leaves_stack_unchanged() -> AndromedaResult<()> {
    let mut stack = SavepointStack::new();
    stack.create("only")?;
    let before = stack.active().to_vec();

    assert_transaction_error(stack.release("missing"));

    assert_eq!(stack.active(), before.as_slice());
    Ok(())
}

#[test]
fn rollback_marker_rejects_zero_savepoint_id() {
    assert_transaction_error(SavepointRollbackMarker::new(SavepointId::new(0), 7));

    let marker = SavepointRollbackMarker {
        savepoint_id: SavepointId::new(0),
        rollback_ordinal: 7,
    };
    assert_transaction_error(marker.validate());
}

#[test]
fn record_operation_revalidates_directly_constructed_public_resource_variants()
-> AndromedaResult<()> {
    let mut write_set = TxWriteSet::new();

    assert_transaction_error(write_set.record_operation(
        WriteSetOperationKind::Insert,
        WriteSetResourceId::Text(String::new()),
        None,
        None,
    ));
    assert_transaction_error(write_set.record_operation(
        WriteSetOperationKind::Insert,
        WriteSetResourceId::Bytes(vec![0; MAX_WRITE_SET_RESOURCE_ID_BYTES + 1]),
        None,
        None,
    ));

    write_set.record_operation(
        WriteSetOperationKind::Insert,
        bytes_resource(b"table/accounts/row/1")?,
        None,
        Some(image(b"value")?),
    )?;
    assert_eq!(write_set.len(), 1);
    Ok(())
}

#[test]
fn record_operation_rejects_invalid_custom_operation_kind_without_mutation() -> AndromedaResult<()>
{
    let mut write_set = TxWriteSet::new();
    write_set.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("table/accounts/row/1")?,
        None,
        Some(image(b"value")?),
    )?;
    let before = write_set.entries().to_vec();

    assert_transaction_error(write_set.record_operation(
        WriteSetOperationKind::Custom(String::new()),
        text_resource("table/accounts/row/2")?,
        None,
        None,
    ));
    assert_transaction_error(write_set.record_operation(
        WriteSetOperationKind::Custom("x".repeat(MAX_WRITE_SET_OPERATION_KIND_BYTES + 1)),
        text_resource("table/accounts/row/3")?,
        None,
        None,
    ));

    assert_eq!(write_set.entries(), before.as_slice());
    Ok(())
}

#[test]
fn oversized_images_are_rejected_before_write_set_insertion() -> AndromedaResult<()> {
    let mut write_set = TxWriteSet::new();
    let oversized = vec![1; MAX_WRITE_SET_IMAGE_BYTES + 1];

    assert_transaction_error(WriteSetImage::try_from_bytes(oversized));

    write_set.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("table/accounts/row/1")?,
        None,
        Some(image(b"small")?),
    )?;
    assert_eq!(write_set.len(), 1);
    Ok(())
}

#[test]
fn rollback_to_future_marker_errors_without_draining_entries() -> AndromedaResult<()> {
    let mut write_set = TxWriteSet::new();
    write_set.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("table/accounts/row/1")?,
        None,
        Some(image(b"value")?),
    )?;
    let before = write_set.entries().to_vec();
    let future_marker = SavepointRollbackMarker::new(SavepointId::new(1), 99)?;

    assert_transaction_error(write_set.rollback_to(future_marker));

    assert_eq!(write_set.entries(), before.as_slice());
    Ok(())
}

#[test]
fn release_rejects_invalid_marker_without_mutating_write_set() -> AndromedaResult<()> {
    let mut write_set = TxWriteSet::new();
    let entry = write_set.record_operation(
        WriteSetOperationKind::Delete,
        text_resource("table/accounts/row/1")?,
        Some(image(b"old")?),
        None,
    )?;
    let invalid_marker = SavepointRollbackMarker {
        savepoint_id: SavepointId::new(0),
        rollback_ordinal: entry.ordinal,
    };

    assert_transaction_error(write_set.release(invalid_marker));

    assert_eq!(write_set.entries(), &[entry]);
    Ok(())
}
