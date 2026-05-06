use std::collections::BTreeMap;

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, InvocationId, TransactionId,
};
use andromeda_observe::{
    ExecutionTransitionTrace, TraceId, TransactionPhaseCode, TransitionReasonCode,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    TransactionTimeout,
    DeadlockVictim,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryRouting {
    NoRetry,
    RetryScheduled { next_attempt: u8, max_attempts: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalTxState {
    Committed,
    RolledBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalTxEvidence {
    pub transaction_id: TransactionId,
    pub terminal_state: TerminalTxState,
    pub durable_wal_fence_lsn: u64,
}

#[derive(Debug, Clone, Default)]
pub struct TerminalTxJournal {
    entries: BTreeMap<TransactionId, TerminalTxEvidence>,
}

impl TerminalTxJournal {
    pub fn record(&mut self, evidence: TerminalTxEvidence) -> AndromedaResult<()> {
        if evidence.durable_wal_fence_lsn == 0 {
            return Err(route_error(
                AndromedaErrorKind::Storage,
                "terminal transaction evidence requires non-zero durable WAL fence",
            ));
        }

        if let Some(existing) = self.entries.get(&evidence.transaction_id)
            && existing.terminal_state != evidence.terminal_state
        {
            return Err(route_error(
                AndromedaErrorKind::Transaction,
                "conflicting terminal state for transaction during recovery",
            ));
        }

        self.entries.insert(evidence.transaction_id, evidence);
        Ok(())
    }

    pub fn evidence_for(&self, transaction_id: TransactionId) -> Option<TerminalTxEvidence> {
        self.entries.get(&transaction_id).copied()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutedTransactionError {
    pub terminal: TerminalTxEvidence,
    pub retry: RetryRouting,
    pub audit: ExecutionTransitionTrace,
}

pub fn route_transaction_error(
    error_kind: ErrorKind,
    trace_id: TraceId,
    invocation_id: InvocationId,
    transaction_id: TransactionId,
    durable_wal_fence_lsn: u64,
    deadlock_attempt: u8,
    deadlock_max_attempts: u8,
    journal: &mut TerminalTxJournal,
) -> AndromedaResult<RoutedTransactionError> {
    if invocation_id.get() == 0 || transaction_id.get() == 0 {
        return Err(route_error(
            AndromedaErrorKind::Contract,
            "transaction error routing requires non-zero invocation and transaction ids",
        ));
    }
    if durable_wal_fence_lsn == 0 {
        return Err(route_error(
            AndromedaErrorKind::Storage,
            "transaction error routing requires non-zero durable WAL fence",
        ));
    }

    let retry = match error_kind {
        ErrorKind::TransactionTimeout => RetryRouting::NoRetry,
        ErrorKind::DeadlockVictim => {
            if !(1..=3).contains(&deadlock_max_attempts) {
                return Err(route_error(
                    AndromedaErrorKind::Contract,
                    "deadlock retry budget must be in 1..=3 attempts",
                ));
            }
            if deadlock_attempt < deadlock_max_attempts {
                RetryRouting::RetryScheduled {
                    next_attempt: deadlock_attempt.saturating_add(1),
                    max_attempts: deadlock_max_attempts,
                }
            } else {
                RetryRouting::NoRetry
            }
        }
    };

    let terminal = TerminalTxEvidence {
        transaction_id,
        terminal_state: TerminalTxState::RolledBack,
        durable_wal_fence_lsn,
    };
    journal.record(terminal)?;

    let (reason_code, reason) = match error_kind {
        ErrorKind::TransactionTimeout => (
            TransitionReasonCode::TRANSACTION_TIMEOUT,
            "transaction timeout routed to durable rollback fence",
        ),
        ErrorKind::DeadlockVictim => (
            TransitionReasonCode::DEADLOCK_VICTIM,
            "deadlock victim routed to durable rollback fence",
        ),
    };

    let audit = ExecutionTransitionTrace {
        trace_id,
        invocation_id,
        request_id: None,
        session_id: None,
        transaction_id: Some(transaction_id),
        completion_code: None,
        prev_phase: Some(TransactionPhaseCode::ACTIVE),
        next_phase: Some(TransactionPhaseCode::ROLLED_BACK),
        durable_lsn: Some(durable_wal_fence_lsn),
        reason_code,
        reason: reason.to_string(),
    };
    audit.validate()?;

    Ok(RoutedTransactionError {
        terminal,
        retry,
        audit,
    })
}

fn route_error(kind: AndromedaErrorKind, message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(kind, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_timeout_is_non_retryable_and_terminal_with_fence() {
        let mut journal = TerminalTxJournal::default();
        let routed = route_transaction_error(
            ErrorKind::TransactionTimeout,
            TraceId::new(1),
            InvocationId::new(2),
            TransactionId::new(3),
            10,
            1,
            3,
            &mut journal,
        )
        .unwrap();

        assert_eq!(routed.retry, RetryRouting::NoRetry);
        assert_eq!(routed.terminal.terminal_state, TerminalTxState::RolledBack);
        assert_eq!(routed.terminal.durable_wal_fence_lsn, 10);
        assert_eq!(
            routed.audit.reason_code,
            TransitionReasonCode::TRANSACTION_TIMEOUT
        );
    }

    #[test]
    fn deadlock_victim_uses_bounded_retry_budget() {
        let mut journal = TerminalTxJournal::default();
        let routed = route_transaction_error(
            ErrorKind::DeadlockVictim,
            TraceId::new(11),
            InvocationId::new(12),
            TransactionId::new(13),
            20,
            1,
            3,
            &mut journal,
        )
        .unwrap();

        assert_eq!(
            routed.retry,
            RetryRouting::RetryScheduled {
                next_attempt: 2,
                max_attempts: 3,
            }
        );
        assert_eq!(
            routed.audit.reason_code,
            TransitionReasonCode::DEADLOCK_VICTIM
        );
    }

    #[test]
    fn timeout_near_commit_cannot_create_dual_terminal_states_post_recovery() {
        let mut journal = TerminalTxJournal::default();
        let tx_id = TransactionId::new(99);

        let timeout_route = route_transaction_error(
            ErrorKind::TransactionTimeout,
            TraceId::new(21),
            InvocationId::new(22),
            tx_id,
            30,
            1,
            3,
            &mut journal,
        )
        .unwrap();
        assert_eq!(
            timeout_route.terminal.terminal_state,
            TerminalTxState::RolledBack
        );

        let conflict = journal.record(TerminalTxEvidence {
            transaction_id: tx_id,
            terminal_state: TerminalTxState::Committed,
            durable_wal_fence_lsn: 31,
        });
        assert!(conflict.is_err());

        let durable = journal
            .evidence_for(tx_id)
            .expect("terminal evidence retained");
        assert_eq!(durable.terminal_state, TerminalTxState::RolledBack);
    }
}
