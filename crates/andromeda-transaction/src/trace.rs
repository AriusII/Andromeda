use andromeda_core::{EngineTimestamp, InvocationId, RequestId, SessionId, TransactionId};
use andromeda_observe::{
    TraceId, TransactionPhaseCode, TransactionTransitionTrace, TransitionReasonCode,
};
use andromeda_transaction_log::Lsn;

use crate::{TransactionState, TransactionStateMachine};

/// Project an `andromeda-transaction` state into the wire-aligned
/// `TransactionPhaseCode` shared by `andromeda-observe`.
pub const fn transaction_phase_code(state: TransactionState) -> TransactionPhaseCode {
    match state {
        TransactionState::Created => TransactionPhaseCode::CREATED,
        TransactionState::Active => TransactionPhaseCode::ACTIVE,
        TransactionState::Committing => TransactionPhaseCode::COMMITTING,
        TransactionState::Committed => TransactionPhaseCode::COMMITTED,
        TransactionState::Failed => TransactionPhaseCode::FAILED,
        TransactionState::RollingBack => TransactionPhaseCode::ROLLING_BACK,
        TransactionState::RolledBack => TransactionPhaseCode::ROLLED_BACK,
        TransactionState::Poisoned => TransactionPhaseCode::POISONED,
        TransactionState::Disposed => TransactionPhaseCode::DISPOSED,
    }
}

/// Correlation envelope for transition projection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransactionTransitionCorrelation {
    pub invocation_id: Option<InvocationId>,
    pub request_id: Option<RequestId>,
    pub session_id: Option<SessionId>,
}

impl TransactionTransitionCorrelation {
    pub const fn empty() -> Self {
        Self {
            invocation_id: None,
            request_id: None,
            session_id: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionTrace {
    pub trace_id: TraceId,
    pub transaction_id: TransactionId,
    pub state: TransactionState,
}

impl TransactionTrace {
    pub const fn new(
        trace_id: TraceId,
        transaction_id: TransactionId,
        state: TransactionState,
    ) -> Self {
        Self {
            trace_id,
            transaction_id,
            state,
        }
    }

    pub const fn from_state_machine(trace_id: TraceId, machine: TransactionStateMachine) -> Self {
        Self {
            trace_id,
            transaction_id: machine.transaction_id(),
            state: machine.state(),
        }
    }

    pub const fn is_terminal(self) -> bool {
        self.state.is_terminal()
    }
}

impl TransactionStateMachine {
    pub fn project_transition(
        self,
        trace_id: TraceId,
        previous_state: TransactionState,
        correlation: TransactionTransitionCorrelation,
        reason_code: TransitionReasonCode,
        reason: impl Into<String>,
    ) -> TransactionTransitionTrace {
        let next_phase = transaction_phase_code(self.state());
        let durable_lsn = match self.state() {
            TransactionState::Committed => self.durable_commit_lsn().map(Lsn::get),
            TransactionState::RolledBack => self.durable_rollback_lsn().map(Lsn::get),
            _ => None,
        };
        TransactionTransitionTrace {
            trace_id,
            transaction_id: self.transaction_id(),
            invocation_id: correlation.invocation_id,
            request_id: correlation.request_id,
            session_id: correlation.session_id,
            prev_phase: transaction_phase_code(previous_state),
            next_phase,
            durable_lsn,
            reason_code,
            reason: reason.into(),
        }
    }
}

/// Trace evidence for terminal lock cleanup (`release_all`).
///
/// Emitted when a transaction's locks are cleaned up after durable terminal
/// evidence (commit or rollback). Captures the cleanup summary and final state.
#[derive(Clone, Debug)]
pub struct LockReleaseAllTrace {
    /// Transaction ID being cleaned up.
    pub tx_id: TransactionId,
    /// Number of resources from which locks were released.
    pub resources_released: usize,
    /// Final transaction state (`Committed` or `RolledBack`).
    pub terminal_state: TransactionState,
    /// Timestamp when cleanup was recorded.
    pub timestamp: EngineTimestamp,
}

impl LockReleaseAllTrace {
    /// Construct a release-all trace.
    pub fn new(
        tx_id: TransactionId,
        resources_released: usize,
        terminal_state: TransactionState,
        timestamp: EngineTimestamp,
    ) -> Self {
        Self {
            tx_id,
            resources_released,
            terminal_state,
            timestamp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_trace_can_snapshot_state_machine() {
        let trace = TransactionTrace::from_state_machine(
            TraceId::new(5),
            TransactionStateMachine::new(TransactionId::new(6)),
        );

        assert_eq!(trace.trace_id, TraceId::new(5));
        assert_eq!(trace.transaction_id, TransactionId::new(6));
        assert_eq!(trace.state, TransactionState::Created);
        assert!(!trace.is_terminal());
    }

    #[test]
    fn lock_release_all_trace_captures_cleanup_summary() {
        let tx_id = TransactionId::new(1);
        let ts = EngineTimestamp::from_unix_millis(1000);
        let trace = LockReleaseAllTrace::new(tx_id, 3, TransactionState::Committed, ts);

        assert_eq!(trace.tx_id, tx_id);
        assert_eq!(trace.resources_released, 3);
        assert_eq!(trace.terminal_state, TransactionState::Committed);
        assert_eq!(trace.timestamp, ts);
    }
}
