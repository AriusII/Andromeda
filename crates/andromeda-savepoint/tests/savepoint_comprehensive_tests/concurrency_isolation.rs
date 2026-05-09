use super::*;

#[test]
fn independent_stacks_allow_same_names_and_local_id_sequences() -> AndromedaResult<()> {
    let mut left = SavepointStack::new();
    let mut right = SavepointStack::new();

    let left_sp = left.create("checkpoint")?;
    let right_sp = right.create("checkpoint")?;

    assert_eq!(left_sp.id.get(), 1);
    assert_eq!(right_sp.id.get(), 1);
    assert_eq!(names(&left), vec!["checkpoint"]);
    assert_eq!(names(&right), vec!["checkpoint"]);
    Ok(())
}

#[test]
fn rollback_on_one_stack_does_not_affect_another_stack() -> AndromedaResult<()> {
    let mut left = SavepointStack::new();
    let mut right = SavepointStack::new();

    left.create("root")?;
    left.create("work")?;
    right.create("root")?;
    right.create("work")?;

    left.rollback_to("root")?;

    assert_eq!(names(&left), vec!["root"]);
    assert_eq!(names(&right), vec!["root", "work"]);
    Ok(())
}

#[test]
fn write_set_rollback_is_transaction_local_to_the_chosen_write_set() -> AndromedaResult<()> {
    let mut left_writes = TxWriteSet::new();
    let mut right_writes = TxWriteSet::new();

    left_writes.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("left/row/1")?,
        None,
        Some(image(b"left-a")?),
    )?;
    right_writes.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("right/row/1")?,
        None,
        Some(image(b"right-a")?),
    )?;

    let marker = SavepointRollbackMarker::new(SavepointId::new(1), left_writes.rollback_ordinal())?;
    let left_later = left_writes.record_operation(
        WriteSetOperationKind::Update,
        text_resource("left/row/1")?,
        Some(image(b"left-a")?),
        Some(image(b"left-b")?),
    )?;
    right_writes.record_operation(
        WriteSetOperationKind::Update,
        text_resource("right/row/1")?,
        Some(image(b"right-a")?),
        Some(image(b"right-b")?),
    )?;

    let undone = left_writes.rollback_to(marker)?;

    assert_eq!(undone, vec![left_later]);
    assert_eq!(text_resources(&left_writes), vec!["left/row/1".to_string()]);
    assert_eq!(
        text_resources(&right_writes),
        vec!["right/row/1".to_string(), "right/row/1".to_string()]
    );
    Ok(())
}

#[test]
fn full_rollback_drains_only_the_selected_write_set() -> AndromedaResult<()> {
    let mut left = TxWriteSet::new();
    let mut right = TxWriteSet::new();

    let left_entry = left.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("left/row/1")?,
        None,
        Some(image(b"left")?),
    )?;
    let right_entry = right.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("right/row/1")?,
        None,
        Some(image(b"right")?),
    )?;

    assert_eq!(left.full_rollback(), vec![left_entry]);

    assert!(left.is_empty());
    assert_eq!(right.entries(), &[right_entry]);
    Ok(())
}

#[test]
fn separate_threads_can_build_independent_stacks_without_shared_state() {
    let handles: Vec<_> = (0..8)
        .map(|worker| {
            std::thread::spawn(move || {
                let mut stack = SavepointStack::new();
                for depth in 0..10 {
                    stack.create(format!("worker_{worker}_sp_{depth}")).unwrap();
                }
                stack.depth()
            })
        })
        .collect();

    for handle in handles {
        assert_eq!(handle.join().unwrap(), 10);
    }
}
