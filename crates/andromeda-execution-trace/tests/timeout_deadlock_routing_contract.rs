use andromeda_execution_trace::{
    ErrorKind, RetryRouting, TerminalTxEvidence, TerminalTxJournal, TerminalTxState,
    route_transaction_error,
};
use andromeda_observability::{TraceId, TransitionReasonCode};
use andromeda_types::{InvocationId, TransactionId};

#[test]
fn transaction_timeout_routes_to_non_retryable_rollback_with_audit_and_fence() {
    let mut journal = TerminalTxJournal::default();

    let routed = route_transaction_error(
        ErrorKind::TransactionTimeout,
        TraceId::new(1001),
        InvocationId::new(1002),
        TransactionId::new(1003),
        9001,
        1,
        3,
        &mut journal,
    )
    .unwrap();

    assert_eq!(routed.retry, RetryRouting::NoRetry);
    assert_eq!(routed.terminal.terminal_state, TerminalTxState::RolledBack);
    assert_eq!(routed.terminal.durable_wal_fence_lsn, 9001);
    assert_eq!(
        routed.audit.reason_code,
        TransitionReasonCode::TRANSACTION_TIMEOUT
    );
}

#[test]
fn deadlock_victim_routes_to_bounded_retry_with_audit_and_terminal_fence() {
    let mut journal = TerminalTxJournal::default();

    let routed = route_transaction_error(
        ErrorKind::DeadlockVictim,
        TraceId::new(1101),
        InvocationId::new(1102),
        TransactionId::new(1103),
        9101,
        2,
        3,
        &mut journal,
    )
    .unwrap();

    assert_eq!(
        routed.retry,
        RetryRouting::RetryScheduled {
            next_attempt: 3,
            max_attempts: 3,
        }
    );
    assert_eq!(routed.terminal.terminal_state, TerminalTxState::RolledBack);
    assert_eq!(
        routed.audit.reason_code,
        TransitionReasonCode::DEADLOCK_VICTIM
    );
}

#[test]
fn timeout_near_commit_recovery_keeps_exactly_one_terminal_state() {
    let mut journal = TerminalTxJournal::default();
    let tx_id = TransactionId::new(1203);

    route_transaction_error(
        ErrorKind::TransactionTimeout,
        TraceId::new(1201),
        InvocationId::new(1202),
        tx_id,
        9201,
        1,
        3,
        &mut journal,
    )
    .unwrap();

    let conflicting_commit = journal.record(TerminalTxEvidence {
        transaction_id: tx_id,
        terminal_state: TerminalTxState::Committed,
        durable_wal_fence_lsn: 9202,
    });

    assert!(conflicting_commit.is_err());
    assert_eq!(
        journal.evidence_for(tx_id).unwrap().terminal_state,
        TerminalTxState::RolledBack
    );
}
