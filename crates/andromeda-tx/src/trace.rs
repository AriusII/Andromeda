use andromeda_core::TransactionId;
use andromeda_observe::TraceId;

use crate::TransactionState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionTrace {
    pub trace_id: TraceId,
    pub transaction_id: TransactionId,
    pub state: TransactionState,
}
