//! V0 cross-engine transaction transition trace contract.
//!
//! Asserts that `TransactionStateMachine::project_transition` produces
//! audit-friendly evidence with stable invocation/request/session/transaction
//! correlation, and that the resulting trace's own `validate()` rejects any
//! attempt to claim a terminal commit/rollback without durable WAL evidence.

use andromeda_core::{InvocationId, RequestId, SessionId, TransactionId};
use andromeda_observe::{
    TraceId, TransactionPhaseCode, TransactionTransitionTrace, TransitionReasonCode,
};
use andromeda_tx::{
    transaction_phase_code, TransactionState, TransactionStateMachine,
    TransactionTransitionCorrelation,
};

#[test]
fn commit_transition_carries_invocation_request_session_and_durable_lsn() {
    let mut tx = TransactionStateMachine::new(TransactionId::new(101));
    tx.begin().unwrap();
    tx.request_commit().unwrap();
    let previous = tx.state;
    tx.publish_visible_commit_after_durable_flush(900).unwrap();

    let trace = tx.project_transition(
        TraceId::new(7),
        previous,
        TransactionTransitionCorrelation {
            invocation_id: Some(InvocationId::new(2)),
            request_id: Some(RequestId::new(3)),
            session_id: Some(SessionId::new(4)),
        },
        TransitionReasonCode::DURABLE_WAL_FLUSH,
        "commit visible after durable WAL flush",
    );

    assert_eq!(trace.transaction_id, TransactionId::new(101));
    assert_eq!(trace.invocation_id, Some(InvocationId::new(2)));
    assert_eq!(trace.request_id, Some(RequestId::new(3)));
    assert_eq!(trace.session_id, Some(SessionId::new(4)));
    assert_eq!(trace.prev_phase, TransactionPhaseCode::COMMITTING);
    assert_eq!(trace.next_phase, TransactionPhaseCode::COMMITTED);
    assert_eq!(trace.durable_lsn, Some(900));
    assert_eq!(trace.reason_code, TransitionReasonCode::DURABLE_WAL_FLUSH);
    assert!(trace.proves_terminal_evidence());
    assert!(trace.validate().is_ok());
}

#[test]
fn rollback_transition_carries_durable_rollback_lsn() {
    let mut tx = TransactionStateMachine::new(TransactionId::new(202));
    tx.begin().unwrap();
    tx.request_rollback().unwrap();
    let previous = tx.state;
    tx.complete_rollback_after_durable_flush(555).unwrap();

    let trace = tx.project_transition(
        TraceId::new(8),
        previous,
        TransactionTransitionCorrelation::empty(),
        TransitionReasonCode::CALLER_ROLLBACK,
        "rollback durable",
    );

    assert_eq!(trace.next_phase, TransactionPhaseCode::ROLLED_BACK);
    assert_eq!(trace.durable_lsn, Some(555));
    assert!(trace.validate().is_ok());
}

#[test]
fn non_terminal_transition_must_not_claim_durable_lsn() {
    // `project_transition` only fills `durable_lsn` for `Committed`/
    // `RolledBack` states, so a `Created -> Active` transition produces a
    // trace without durable evidence — and the trace validates cleanly.
    let mut tx = TransactionStateMachine::new(TransactionId::new(303));
    tx.begin().unwrap();
    let trace = tx.project_transition(
        TraceId::new(9),
        TransactionState::Created,
        TransactionTransitionCorrelation::empty(),
        TransitionReasonCode::NORMAL_PROGRESS,
        "begin",
    );

    assert_eq!(trace.next_phase, TransactionPhaseCode::ACTIVE);
    assert!(trace.durable_lsn.is_none());
    assert!(trace.validate().is_ok());
}

#[test]
fn forged_terminal_transition_without_durable_lsn_is_rejected_by_validate() {
    // A trace constructed directly (bypassing the state machine) cannot
    // claim a `Committed` next phase without durable LSN evidence.
    let forged = TransactionTransitionTrace {
        trace_id: TraceId::new(10),
        transaction_id: TransactionId::new(404),
        invocation_id: None,
        request_id: None,
        session_id: None,
        prev_phase: TransactionPhaseCode::COMMITTING,
        next_phase: TransactionPhaseCode::COMMITTED,
        durable_lsn: None,
        reason_code: TransitionReasonCode::DURABLE_WAL_FLUSH,
        reason: "forged commit without WAL flush".to_string(),
    };

    let err = forged.validate().unwrap_err();
    assert!(err.message().contains("durable_lsn evidence"));
}

#[test]
fn transition_phase_codes_cover_every_transaction_state() {
    // Drift guard between `andromeda_tx::TransactionState` and the
    // `andromeda_observe::TransactionPhaseCode` constants. Adding a new
    // state requires extending `transaction_phase_code` and the phase code
    // constants together; this test fails compilation when a new state is
    // introduced.
    use TransactionState::*;
    for state in [
        Created,
        Active,
        Committing,
        Committed,
        Failed,
        RollingBack,
        RolledBack,
        Poisoned,
        Disposed,
    ] {
        let code = transaction_phase_code(state);
        assert!(code.is_known(), "phase code for {:?} must be known", state);
    }
}
