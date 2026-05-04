use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, InvocationId};
use andromeda_observe::TraceId;
use andromeda_storage::Lsn;
use andromeda_tx::TransactionState;

use crate::{CompletionStatus, InvocationCompletion};

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
        if transaction_state != TransactionState::Committed {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "committed completion requires committed transaction state",
            ));
        }

        if durable_lsn.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "committed completion requires nonzero durable LSN evidence",
            ));
        }

        Ok(InvocationCompletion {
            invocation_id,
            status: CompletionStatus::Committed,
            rows_affected: Some(rows_affected),
            transaction_state: Some(transaction_state),
            durable_lsn: Some(durable_lsn),
            trace_id,
        })
    }

    pub fn rolled_back(
        invocation_id: InvocationId,
        transaction_state: TransactionState,
        durable_lsn: Lsn,
        trace_id: TraceId,
    ) -> AndromedaResult<InvocationCompletion> {
        if transaction_state != TransactionState::RolledBack {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "rolled-back completion requires rolled-back transaction state",
            ));
        }

        if durable_lsn.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "rolled-back completion requires nonzero durable LSN evidence",
            ));
        }

        Ok(InvocationCompletion {
            invocation_id,
            status: CompletionStatus::RolledBack,
            rows_affected: Some(0),
            transaction_state: Some(transaction_state),
            durable_lsn: Some(durable_lsn),
            trace_id,
        })
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

    /// Build a `Poisoned` completion that has been routed through a durable
    /// rollback. Mirrors the executor invariant that poison failures must
    /// transit `TransactionState::Poisoned` before reaching durable
    /// `RolledBack`. Callers MUST supply the rolled-back transaction state
    /// and the durable WAL LSN proving the rollback is durable. Use
    /// `rejected` instead for poison failures detected before any
    /// transaction begin.
    pub fn poisoned_after_rollback(
        invocation_id: InvocationId,
        transaction_state: TransactionState,
        durable_lsn: Lsn,
        trace_id: TraceId,
    ) -> AndromedaResult<InvocationCompletion> {
        if transaction_state != TransactionState::RolledBack {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "poisoned-after-rollback completion requires rolled-back transaction state",
            ));
        }
        if durable_lsn.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "poisoned-after-rollback completion requires nonzero durable LSN evidence",
            ));
        }
        Ok(InvocationCompletion {
            invocation_id,
            status: CompletionStatus::Poisoned,
            rows_affected: Some(0),
            transaction_state: Some(transaction_state),
            durable_lsn: Some(durable_lsn),
            trace_id,
        })
    }

    /// Build a `FailedBeforeTransaction` completion with no transaction
    /// evidence. This is the only legal projection of a runtime failure that
    /// occurred prior to `Begin`; failures observed after `Begin` MUST be
    /// routed through the rolled-back path so the transaction terminal
    /// invariant is preserved.
    pub fn failed_before_transaction(
        invocation_id: InvocationId,
        trace_id: TraceId,
    ) -> InvocationCompletion {
        InvocationCompletion {
            invocation_id,
            status: CompletionStatus::FailedBeforeTransaction,
            rows_affected: None,
            transaction_state: None,
            durable_lsn: None,
            trace_id,
        }
    }
}
