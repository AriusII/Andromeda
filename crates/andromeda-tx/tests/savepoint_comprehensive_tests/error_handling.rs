use super::*;

// TASK 4 — Error Handling & Rollback Tests

/// TC-EH-0001
/// fail() transitions Active → Failed; savepoint operations are then rejected.
#[test]
fn eh_fail_event_blocks_savepoint_operations() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.create_savepoint(tx, "before_fail").unwrap();

    mgr.fail(tx).unwrap();

    let snap = mgr.snapshot(tx).unwrap().unwrap();
    assert_eq!(snap.state_machine.state, TransactionState::Failed);

    // All savepoint ops must be rejected after fail.
    assert!(mgr.create_savepoint(tx, "after_fail").is_err());
    assert!(mgr.rollback_to_savepoint(tx, "before_fail").is_err());
    assert!(mgr.release_savepoint(tx, "before_fail").is_err());
}

/// TC-EH-0002
/// Failed transaction can progress to RollingBack via request_rollback.
#[test]
fn eh_failed_tx_can_transition_to_rolling_back() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.fail(tx).unwrap();

    // Failed → RollingBack is a legal transition.
    mgr.request_rollback(tx).unwrap();
    let snap = mgr.snapshot(tx).unwrap().unwrap();
    assert_eq!(snap.state_machine.state, TransactionState::RollingBack);
}

/// TC-EH-0003
/// Failed → RolledBack requires a durable WAL LSN (WAL-before-visibility).
#[test]
fn eh_failed_tx_rollback_completion_requires_durable_wal() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.fail(tx).unwrap();
    mgr.request_rollback(tx).unwrap();

    // Attempt RollbackComplete without a durable LSN — must fail.
    // We directly use the state machine to probe this invariant.
    let snap = mgr.snapshot(tx).unwrap().unwrap();
    let mut machine = snap.state_machine;
    let err = machine
        .apply(andromeda_tx::TransactionEvent::RollbackComplete)
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-EH-0004
/// Poisoned transaction must rollback before disposal (INV-SP-12).
#[test]
fn eh_poisoned_tx_must_rollback_before_disposal() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.poison(tx).unwrap();

    // Cannot commit from Poisoned.
    let err = mgr.request_commit(tx).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);

    // Cannot dispose from Poisoned.
    let err = mgr.dispose(tx).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);

    // Must rollback.
    mgr.request_rollback(tx).unwrap();
    mgr.rollback_durable(tx, 55).unwrap();
    mgr.dispose(tx).unwrap();
    assert_eq!(mgr.live_count().unwrap(), 0);
}

/// TC-EH-0005
/// rollback_to_savepoint on a Failed transaction returns a clean error; no
/// state corruption occurs.
#[test]
fn eh_rollback_to_on_failed_tx_returns_clean_error_no_corruption() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.create_savepoint(tx, "safe_point").unwrap();

    mgr.fail(tx).unwrap();

    let err = mgr.rollback_to_savepoint(tx, "safe_point").unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);

    // Tx is still Failed — not corrupted to another state.
    let snap = mgr.snapshot(tx).unwrap().unwrap();
    assert_eq!(snap.state_machine.state, TransactionState::Failed);
}

/// TC-EH-0006
/// release_savepoint on a Failed transaction returns a clean error.
#[test]
fn eh_release_on_failed_tx_returns_clean_error() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.create_savepoint(tx, "sp").unwrap();
    mgr.fail(tx).unwrap();

    let err = mgr.release_savepoint(tx, "sp").unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-EH-0007
/// Rolling back to a nonexistent savepoint does not affect the outer savepoint.
#[test]
fn eh_rollback_to_nonexistent_sp_does_not_affect_outer_savepoint() {
    let mut stack = SavepointStack::new();
    stack.create("outer").unwrap();
    stack.create("inner").unwrap();

    let depth_before = stack.depth();
    let err = stack.rollback_to("ghost_sp").unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);

    // Stack must be unmodified.
    assert_eq!(stack.depth(), depth_before);
    let names: Vec<&str> = stack.active().iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["outer", "inner"]);
}

/// TC-EH-0008
/// Releasing a nonexistent savepoint does not alter the stack.
#[test]
fn eh_release_nonexistent_sp_does_not_alter_stack() {
    let mut stack = SavepointStack::new();
    stack.create("real_sp").unwrap();
    let depth_before = stack.depth();

    let err = stack.release("phantom").unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(stack.depth(), depth_before);
}

/// TC-EH-0009
/// After a failed nested rollback, the parent savepoint remains operable.
#[test]
fn eh_failed_inner_rollback_parent_sp_remains_operable() {
    let mut stack = SavepointStack::new();
    stack.create("parent").unwrap();
    stack.create("child").unwrap();

    // Attempt to rollback to a ghost — fails cleanly.
    stack.rollback_to("ghost").unwrap_err();

    // Parent rollback must still succeed.
    let ev = stack.rollback_to("parent").unwrap();
    assert_eq!(ev.target.name, "parent");
    assert_eq!(ev.discarded_descendants.len(), 1);
}

/// TC-EH-0010
/// Commit requires durable WAL LSN before Committed state.
#[test]
fn eh_commit_without_wal_lsn_is_rejected() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.request_commit(tx).unwrap();

    // commit_durable with LSN=0 must be rejected.
    let err = mgr.commit_durable(tx, 0).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-EH-0011
/// Rollback completion requires durable WAL LSN.
#[test]
fn eh_rollback_completion_without_wal_lsn_is_rejected() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.request_rollback(tx).unwrap();

    let err = mgr.rollback_durable(tx, 0).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-EH-0012
/// Commit on a RollingBack transaction is rejected.
#[test]
fn eh_commit_on_rolling_back_tx_is_rejected() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.request_rollback(tx).unwrap();

    let err = mgr.request_commit(tx).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-EH-0013
/// dispose() on an Active transaction (non-terminal) is rejected.
#[test]
fn eh_dispose_on_active_tx_is_rejected() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    let err = mgr.dispose(tx).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-EH-0014
/// double-begin: beginning a transaction twice is safe because each call
/// allocates a fresh id from a monotonic allocator — no collision possible.
#[test]
fn eh_two_begins_allocate_different_ids() {
    let mgr = TransactionManager::new();
    let tx1 = mgr.begin().unwrap();
    let tx2 = mgr.begin().unwrap();
    assert_ne!(tx1, tx2);
    assert!(tx2.get() > tx1.get());
    assert_eq!(mgr.live_count().unwrap(), 2);
}

/// TC-EH-0015
/// fail() on a Created transaction (before begin) is rejected.
#[test]
fn eh_fail_on_created_state_is_rejected() {
    // Direct state machine test — Created → Failed is not a legal transition.
    let state = andromeda_tx::TransactionState::Created;
    let err = state
        .apply(andromeda_tx::TransactionEvent::Fail)
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-EH-0016
/// TransactionTrace correctly captures terminal state after commit.
#[test]
fn eh_trace_captures_terminal_state_after_commit() {
    use andromeda_observe::TraceId;
    use andromeda_tx::{TransactionStateMachine, TransactionTrace};

    let mut machine = TransactionStateMachine::new(TransactionId::new(1));
    machine.begin().unwrap();
    machine.request_commit().unwrap();
    machine
        .publish_visible_commit_after_durable_flush(999)
        .unwrap();

    let trace = TransactionTrace::from_state_machine(TraceId::new(42), machine);
    assert!(trace.is_terminal());
    assert_eq!(trace.state, TransactionState::Committed);
}

/// TC-EH-0017
/// project_transition produces non-terminal trace for Active → Committing.
#[test]
fn eh_trace_non_terminal_for_active_to_committing() {
    use andromeda_observe::{TraceId, TransitionReasonCode};
    use andromeda_tx::{TransactionStateMachine, TransactionTransitionCorrelation};

    let mut machine = TransactionStateMachine::new(TransactionId::new(2));
    machine.begin().unwrap();
    let prev = machine.state;
    machine.request_commit().unwrap();

    let trace = machine.project_transition(
        TraceId::new(1),
        prev,
        TransactionTransitionCorrelation::empty(),
        TransitionReasonCode::NORMAL_PROGRESS,
        "commit requested",
    );

    assert_eq!(trace.durable_lsn, None);
    assert!(trace.validate().is_ok());
}

/// TC-EH-0018
/// project_transition for RolledBack carries durable_lsn evidence.
#[test]
fn eh_trace_rolled_back_carries_durable_lsn() {
    use andromeda_observe::{TraceId, TransitionReasonCode};
    use andromeda_tx::{TransactionStateMachine, TransactionTransitionCorrelation};

    let mut machine = TransactionStateMachine::new(TransactionId::new(3));
    machine.begin().unwrap();
    machine.request_rollback().unwrap();
    let prev = machine.state;
    machine.complete_rollback_after_durable_flush(1234).unwrap();

    let trace = machine.project_transition(
        TraceId::new(2),
        prev,
        TransactionTransitionCorrelation::empty(),
        TransitionReasonCode::DURABLE_WAL_FLUSH,
        "rollback durable",
    );

    assert_eq!(trace.durable_lsn, Some(1234));
    assert!(trace.proves_terminal_evidence());
    assert!(trace.validate().is_ok());
}
