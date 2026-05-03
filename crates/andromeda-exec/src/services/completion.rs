use andromeda_core::InvocationId;
use andromeda_observe::TraceId;
use andromeda_storage::Lsn;
use andromeda_tx::TransactionState;

use crate::{CompletionStatus, InvocationCompletion};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompletionMappingService;

impl CompletionMappingService {
    pub const fn committed(
        invocation_id: InvocationId,
        rows_affected: u64,
        transaction_state: TransactionState,
        durable_lsn: Lsn,
        trace_id: TraceId,
    ) -> InvocationCompletion {
        InvocationCompletion {
            invocation_id,
            status: CompletionStatus::Committed,
            rows_affected: Some(rows_affected),
            transaction_state: Some(transaction_state),
            durable_lsn: Some(durable_lsn),
            trace_id,
        }
    }

    pub fn rejected(
        invocation_id: InvocationId,
        status: CompletionStatus,
        trace_id: TraceId,
    ) -> InvocationCompletion {
        InvocationCompletion {
            invocation_id,
            status,
            rows_affected: None,
            transaction_state: None,
            durable_lsn: None,
            trace_id,
        }
    }
}
