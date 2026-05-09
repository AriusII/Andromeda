use andromeda_mvcc::TransactionStatus;

use crate::state::TransactionStateMachine;

/// Snapshot of a transaction known to the manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionRecord {
    pub state_machine: TransactionStateMachine,
    pub status: TransactionStatus,
    pub savepoint_depth: usize,
}
