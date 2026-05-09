use super::*;

#[test]
fn fresh_stack_after_recovery_contains_no_live_savepoints() -> AndromedaResult<()> {
    let mut pre_crash = SavepointStack::new();
    pre_crash.create("before_crash")?;
    pre_crash.create("mid_crash")?;
    assert_eq!(pre_crash.depth(), 2);

    let recovered = SavepointStack::new();

    assert!(recovered.is_empty());
    assert_eq!(recovered.depth(), 0);
    Ok(())
}

#[test]
fn discarding_incomplete_transaction_state_drops_savepoints_and_write_set() -> AndromedaResult<()> {
    let mut stack = SavepointStack::new();
    let mut write_set = TxWriteSet::new();

    stack.create("checkpoint")?;
    write_set.record_operation(
        WriteSetOperationKind::Insert,
        text_resource("table/recovery/row/1")?,
        None,
        Some(image(b"value")?),
    )?;

    stack = SavepointStack::new();
    write_set = TxWriteSet::new();

    assert!(stack.is_empty());
    assert!(write_set.is_empty());
    Ok(())
}

#[test]
fn savepoint_create_perf_gate_for_large_stack() -> AndromedaResult<()> {
    let start = std::time::Instant::now();
    let mut stack = SavepointStack::new();

    for i in 0..1_000 {
        stack.create(format!("sp_{i}"))?;
    }

    assert_eq!(stack.depth(), 1_000);
    assert!(
        start.elapsed().as_millis() < 10_000,
        "creating 1000 savepoints exceeded the 10 second owner-test gate"
    );
    Ok(())
}

#[test]
fn savepoint_rollback_perf_gate_for_nested_stack() -> AndromedaResult<()> {
    let start = std::time::Instant::now();

    for iteration in 0..100 {
        let mut stack = SavepointStack::new();
        for depth in 0..50 {
            stack.create(format!("s_{iteration}_{depth}"))?;
        }
        stack.rollback_to(&format!("s_{iteration}_10"))?;
        assert_eq!(stack.depth(), 11);
    }

    assert!(
        start.elapsed().as_millis() < 50_000,
        "100 nested rollback operations exceeded the 50 second owner-test gate"
    );
    Ok(())
}

#[test]
fn write_set_rollback_perf_gate_for_large_transaction_local_set() -> AndromedaResult<()> {
    let mut write_set = TxWriteSet::new();

    for i in 0..500 {
        write_set.record_operation(
            WriteSetOperationKind::Insert,
            text_resource(&format!("table/perf/row/{i}"))?,
            None,
            Some(image(b"value")?),
        )?;
    }
    let marker = SavepointRollbackMarker::new(SavepointId::new(1), write_set.rollback_ordinal())?;
    for i in 500..1_000 {
        write_set.record_operation(
            WriteSetOperationKind::Update,
            text_resource(&format!("table/perf/row/{i}"))?,
            Some(image(b"before")?),
            Some(image(b"after")?),
        )?;
    }

    let start = std::time::Instant::now();
    let undo_entries = write_set.rollback_to(marker)?;

    assert_eq!(undo_entries.len(), 500);
    assert_eq!(write_set.len(), 500);
    assert!(
        start.elapsed().as_millis() < 5_000,
        "rollback of 500 write-set entries exceeded the 5 second owner-test gate"
    );
    Ok(())
}

#[test]
fn clear_releases_active_depth_for_large_stack() -> AndromedaResult<()> {
    let mut stack = SavepointStack::new();
    for i in 0..1_000 {
        stack.create(format!("sp_{i}"))?;
    }

    stack.clear();

    assert_eq!(stack.depth(), 0);
    assert!(stack.is_empty());
    Ok(())
}
