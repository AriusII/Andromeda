use std::collections::BTreeMap;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observability::{
    ExecutionTransitionTrace, TraceId, TransactionPhaseCode, TransitionReasonCode,
};
use andromeda_retry::{ExecutionErrorRetryability, RetryAttempt, RetryDecision, RetryPolicy};
use andromeda_types::{InvocationId, TransactionId};

/// Transaction-specific error kinds that drive the routing decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    TransactionTimeout,
    DeadlockVictim,
}

impl ErrorKind {
    /// Map the transaction-layer error kind to its [`AndromedaErrorKind`].
    ///
    /// This provides the G4 explicit conversion bridge between the local
    /// `ErrorKind` (used in routing) and the canonical `AndromedaErrorKind`
    /// (used in error reporting and the doctrinal classification matrix).
    ///
    /// Both `TransactionTimeout` and `DeadlockVictim` produce
    /// `AndromedaErrorKind::Transaction` (AE0011). Timeout specifically
    /// also matches `AndromedaErrorKind::Timeout` (AE0010) at the QUIC/stream
    /// layer, but at the transaction-routing layer the unified kind is
    /// `Transaction`.
    pub const fn as_andromeda_kind(self) -> AndromedaErrorKind {
        match self {
            Self::TransactionTimeout => AndromedaErrorKind::Timeout,
            Self::DeadlockVictim => AndromedaErrorKind::Transaction,
        }
    }

    /// Classify the transaction-layer retryability of this error kind.
    ///
    /// Deadlock victims are [`ExecutionErrorRetryability::Retryable`] within
    /// the transaction-routing layer (before the error surfaces). Timeouts are
    /// always [`ExecutionErrorRetryability::NonRetryable`].
    ///
    /// This resolves the G3 reconciliation: `ErrorRetryability` (outer level)
    /// classifies `Transaction` as `Persistent`, while this function classifies
    /// deadlock as `Retryable` at the inner transaction level. The two
    /// classifiers are complementary, not contradictory.
    pub const fn execution_retryability(self) -> ExecutionErrorRetryability {
        ExecutionErrorRetryability::for_transaction_error(matches!(self, Self::DeadlockVictim))
    }
}

impl From<ErrorKind> for AndromedaErrorKind {
    /// Convert a transaction-routing [`ErrorKind`] to its canonical
    /// [`AndromedaErrorKind`]. Provides the G4 explicit bridge between
    /// the two separate type domains.
    fn from(kind: ErrorKind) -> Self {
        kind.as_andromeda_kind()
    }
}

/// Deadlock retry policy used by `route_transaction_error`.
///
/// Deadlock victims receive up to 3 retries with aggressive backoff (50ms
/// initial, 1s max). This policy is intentionally conservative: deadlock
/// resolution is bounded and the transaction body must be idempotent.
const DEADLOCK_RETRY_POLICY: RetryPolicy = RetryPolicy::aggressive();

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
    /// Audit record for a scheduled retry attempt.
    ///
    /// Present only when `retry == RetryRouting::RetryScheduled`. This record
    /// SHOULD be emitted to the [`AuditLedger`] before the retry is executed
    /// so that every retry attempt is observable in the durable trace.
    ///
    /// `None` when `retry == RetryRouting::NoRetry` (timeout or deadlock
    /// budget exhausted).
    pub retry_attempt: Option<RetryAttempt>,
}

// Keep the evidence fields flat so callers pass the audited routing context explicitly.
#[allow(clippy::too_many_arguments)]
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
        },
    };

    // Construct a RetryAttempt for every scheduled retry. This provides the
    // G10 audit integration: callers SHOULD emit `retry_attempt` to the
    // AuditLedger before executing the retry.
    let retry_attempt = match retry {
        RetryRouting::RetryScheduled {
            next_attempt,
            max_attempts: _,
        } => {
            let next_u32 = u32::from(next_attempt);
            let delay_ms = DEADLOCK_RETRY_POLICY
                .delay_for_attempt(next_u32)
                .unwrap_or(50);
            Some(RetryAttempt {
                trace_id,
                attempt_number: u32::from(deadlock_attempt),
                error_kind: AndromedaErrorKind::Transaction,
                error_message: format!(
                    "deadlock victim: scheduling transaction retry (attempt {} → {})",
                    deadlock_attempt, next_attempt
                ),
                next_delay_ms: Some(delay_ms),
                decision: RetryDecision::RetryAfter {
                    next_attempt: next_u32,
                    delay_ms,
                },
            })
        },
        RetryRouting::NoRetry => None,
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
        retry_attempt,
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
        // Timeout is not retried; no retry_attempt record.
        assert!(routed.retry_attempt.is_none());
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

    /// G4: `From<ErrorKind> for AndromedaErrorKind` conversion is explicit and
    /// deterministic. Timeout → `Timeout` (AE0010), Deadlock → `Transaction`
    /// (AE0011).
    #[test]
    fn error_kind_converts_to_andromeda_kind_with_deterministic_codes() {
        let timeout_kind: AndromedaErrorKind = ErrorKind::TransactionTimeout.into();
        assert_eq!(timeout_kind, AndromedaErrorKind::Timeout);
        assert_eq!(timeout_kind.code(), "AE0010");

        let deadlock_kind: AndromedaErrorKind = ErrorKind::DeadlockVictim.into();
        assert_eq!(deadlock_kind, AndromedaErrorKind::Transaction);
        assert_eq!(deadlock_kind.code(), "AE0011");
    }

    /// G3: `ErrorKind::DeadlockVictim` is `Retryable` at the transaction layer;
    /// `ErrorKind::TransactionTimeout` is `NonRetryable`.
    #[test]
    fn error_kind_execution_retryability_reconciles_g3_contradiction() {
        assert_eq!(
            ErrorKind::DeadlockVictim.execution_retryability(),
            ExecutionErrorRetryability::Retryable,
            "deadlock victim must be Retryable at transaction layer"
        );
        assert_eq!(
            ErrorKind::TransactionTimeout.execution_retryability(),
            ExecutionErrorRetryability::NonRetryable,
            "transaction timeout must be NonRetryable at transaction layer"
        );
    }

    /// G10: `RetryAttempt` is constructed and available for each scheduled
    /// deadlock retry. The audit record carries the correct trace_id, attempt
    /// number, and next delay.
    #[test]
    fn deadlock_retry_emits_retry_attempt_with_audit_data() {
        let mut journal = TerminalTxJournal::default();
        let trace_id = TraceId::new(31);

        let routed = route_transaction_error(
            ErrorKind::DeadlockVictim,
            trace_id,
            InvocationId::new(32),
            TransactionId::new(33),
            40,
            1, // first attempt
            3,
            &mut journal,
        )
        .unwrap();

        assert!(
            matches!(routed.retry, RetryRouting::RetryScheduled { .. }),
            "deadlock at attempt 1 of 3 should schedule a retry"
        );

        let attempt = routed
            .retry_attempt
            .expect("scheduled retry must produce a RetryAttempt");
        assert_eq!(attempt.trace_id, trace_id);
        assert_eq!(attempt.attempt_number, 1);
        assert_eq!(attempt.error_kind, AndromedaErrorKind::Transaction);
        assert!(
            attempt.next_delay_ms.is_some(),
            "retry attempt must carry a computed backoff delay"
        );
        assert!(
            attempt.decision.is_retry(),
            "retry attempt decision must be RetryAfter"
        );
    }

    /// G10: When the deadlock budget is exhausted, no `RetryAttempt` is
    /// produced (the error is terminal).
    #[test]
    fn deadlock_budget_exhausted_produces_no_retry_attempt() {
        let mut journal = TerminalTxJournal::default();

        let routed = route_transaction_error(
            ErrorKind::DeadlockVictim,
            TraceId::new(41),
            InvocationId::new(42),
            TransactionId::new(43),
            50,
            3, // attempt == max → budget exhausted
            3,
            &mut journal,
        )
        .unwrap();

        assert_eq!(routed.retry, RetryRouting::NoRetry);
        assert!(
            routed.retry_attempt.is_none(),
            "exhausted deadlock budget must not produce a RetryAttempt"
        );
    }

    /// G10: Persistent error (timeout) never produces a `RetryAttempt`.
    #[test]
    fn persistent_error_never_retries_and_has_no_retry_attempt() {
        let mut journal = TerminalTxJournal::default();

        let routed = route_transaction_error(
            ErrorKind::TransactionTimeout,
            TraceId::new(51),
            InvocationId::new(52),
            TransactionId::new(53),
            60,
            1,
            3,
            &mut journal,
        )
        .unwrap();

        assert_eq!(routed.retry, RetryRouting::NoRetry);
        assert!(routed.retry_attempt.is_none());
    }
}
