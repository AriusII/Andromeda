use andromeda_core::TransactionId;
use andromeda_observe::TraceId;

use crate::{TransactionState, TransactionStateMachine};

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
}
