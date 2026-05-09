use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, InvocationId};
use andromeda_observe::TraceId;
use andromeda_result_stream::{CompletionStatus, InvocationCompletion};
use andromeda_storage::Lsn;
use andromeda_transaction::TransactionState;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompletionMappingService;

impl CompletionMappingService {
    pub fn committed(
        invocation_id: InvocationId,
        rows_affected: u64,
        transaction_state: TransactionState,
        durable_lsn: Lsn,
        trace_id: TraceId,
    ) -> AndromedaResult<InvocationCompletion> {
        InvocationCompletion::committed(
            invocation_id,
            rows_affected,
            transaction_state,
            durable_lsn,
            trace_id,
        )
    }

    pub fn rolled_back(
        invocation_id: InvocationId,
        transaction_state: TransactionState,
        durable_lsn: Lsn,
        trace_id: TraceId,
    ) -> AndromedaResult<InvocationCompletion> {
        InvocationCompletion::rolled_back(invocation_id, transaction_state, durable_lsn, trace_id)
    }

    pub fn rejected(
        invocation_id: InvocationId,
        status: CompletionStatus,
        trace_id: TraceId,
    ) -> AndromedaResult<InvocationCompletion> {
        if status.is_transactional_terminal() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "pre-transaction completion cannot use transactional terminal status",
            ));
        }
        InvocationCompletion::pre_transaction(invocation_id, status, trace_id)
    }

    /// Build the terminal completion for a poisoned path that has been routed
    /// through a durable rollback. Poison is an intermediate transaction state;
    /// after rollback WAL evidence is durable, the emitted completion is
    /// `RolledBack` so the invocation cannot be mistaken for a visible commit.
    /// Use `rejected` instead for poison failures detected before any
    /// transaction begin.
    pub fn poisoned_after_rollback(
        invocation_id: InvocationId,
        transaction_state: TransactionState,
        durable_lsn: Lsn,
        trace_id: TraceId,
    ) -> AndromedaResult<InvocationCompletion> {
        InvocationCompletion::rolled_back(invocation_id, transaction_state, durable_lsn, trace_id)
    }

    /// Build a `FailedBeforeTransaction` completion with no transaction
    /// evidence. This is the only legal projection of a runtime failure that
    /// occurred prior to `Begin`; failures observed after `Begin` MUST be
    /// routed through the rolled-back path so the transaction terminal
    /// invariant is preserved.
    pub fn failed_before_transaction(
        invocation_id: InvocationId,
        trace_id: TraceId,
    ) -> AndromedaResult<InvocationCompletion> {
        InvocationCompletion::pre_transaction(
            invocation_id,
            CompletionStatus::FailedBeforeTransaction,
            trace_id,
        )
    }
}
