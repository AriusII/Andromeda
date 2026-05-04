use andromeda_core::{InvocationId, RequestId, SessionId, TransactionId};
use andromeda_observe::{
    TraceId, TransactionPhaseCode, TransactionTransitionTrace, TransitionReasonCode,
};

use crate::{TransactionState, TransactionStateMachine};

/// Project an `andromeda-tx` `TransactionState` into the wire-aligned
/// `TransactionPhaseCode` shared by `andromeda-observe`. The mapping is the
/// canonical bridge that keeps `andromeda-observe` independent of the
/// `andromeda-tx` enum while preserving stable observability codes.
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

/// Cross-engine correlation envelope passed by callers that want to project a
/// transaction state-machine transition into a `TransactionTransitionTrace`.
/// Keeps invocation/request/session IDs grouped without forcing positional
/// arguments on every projection call.
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
            transaction_id: machine.transaction_id,
            state: machine.state,
        }
    }

    pub const fn is_terminal(self) -> bool {
        self.state.is_terminal()
    }
}

impl TransactionStateMachine {
    /// Project a single state-machine transition into an audit-friendly
    /// [`TransactionTransitionTrace`] carrying stable correlation evidence.
    ///
    /// `previous_state` is the state the machine was in before the most
    /// recent `apply` call; the projection records both endpoints so a sink
    /// can reconstruct the transition without consulting prior traces.
    /// `durable_lsn` is required for terminal commit and rollback transitions
    /// (the trace's own `validate()` enforces this), and must be `None` for
    /// non-terminal transitions to avoid forging durability claims.
    pub fn project_transition(
        self,
        trace_id: TraceId,
        previous_state: TransactionState,
        correlation: TransactionTransitionCorrelation,
        reason_code: TransitionReasonCode,
        reason: impl Into<String>,
    ) -> TransactionTransitionTrace {
        let next_phase = transaction_phase_code(self.state);
        let durable_lsn = match self.state {
            TransactionState::Committed => self.durable_commit_lsn,
            TransactionState::RolledBack => self.durable_rollback_lsn,
            _ => None,
        };
        TransactionTransitionTrace {
            trace_id,
            transaction_id: self.transaction_id,
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
    fn transition_projection_carries_correlation_and_durable_lsn_for_commit() {
        let mut tx = TransactionStateMachine::new(TransactionId::new(11));
        tx.begin().unwrap();
        tx.request_commit().unwrap();
        let previous = tx.state;
        tx.publish_visible_commit_after_durable_flush(900).unwrap();

        let correlation = TransactionTransitionCorrelation {
            invocation_id: Some(InvocationId::new(2)),
            request_id: Some(RequestId::new(3)),
            session_id: Some(SessionId::new(4)),
        };
        let trace = tx.project_transition(
            TraceId::new(7),
            previous,
            correlation,
            TransitionReasonCode::DURABLE_WAL_FLUSH,
            "commit visible after durable WAL flush",
        );

        assert_eq!(trace.transaction_id, TransactionId::new(11));
        assert_eq!(trace.invocation_id, Some(InvocationId::new(2)));
        assert_eq!(trace.request_id, Some(RequestId::new(3)));
        assert_eq!(trace.session_id, Some(SessionId::new(4)));
        assert_eq!(trace.prev_phase, TransactionPhaseCode::COMMITTING);
        assert_eq!(trace.next_phase, TransactionPhaseCode::COMMITTED);
        assert_eq!(trace.durable_lsn, Some(900));
        assert!(trace.proves_terminal_evidence());
        assert!(trace.validate().is_ok());
    }

    #[test]
    fn transition_projection_omits_lsn_on_non_terminal_phases() {
        let mut tx = TransactionStateMachine::new(TransactionId::new(12));
        tx.begin().unwrap();
        let previous = TransactionState::Created;

        let trace = tx.project_transition(
            TraceId::new(8),
            previous,
            TransactionTransitionCorrelation::empty(),
            TransitionReasonCode::NORMAL_PROGRESS,
            "begin",
        );

        assert_eq!(trace.next_phase, TransactionPhaseCode::ACTIVE);
        assert_eq!(trace.durable_lsn, None);
        assert!(trace.validate().is_ok());
    }

    #[test]
    fn transition_projection_into_committed_without_durable_lsn_is_unreachable_via_state_machine() {
        // The state machine itself refuses to enter `Committed` without a
        // durable commit LSN, so the projected trace never observes a
        // terminal commit phase missing LSN evidence. We assert the
        // negative path on a synthetic trace constructed directly to prove
        // the validator rejects it.
        let forged = TransactionTransitionTrace {
            trace_id: TraceId::new(9),
            transaction_id: TransactionId::new(13),
            invocation_id: None,
            request_id: None,
            session_id: None,
            prev_phase: TransactionPhaseCode::COMMITTING,
            next_phase: TransactionPhaseCode::COMMITTED,
            durable_lsn: None,
            reason_code: TransitionReasonCode::DURABLE_WAL_FLUSH,
            reason: "forged commit without WAL".to_string(),
        };
        let err = forged.validate().unwrap_err();
        assert!(err.message().contains("durable_lsn evidence"));
    }
}
