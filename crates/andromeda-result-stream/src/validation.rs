use crate::{CompletionStatus, InvocationCompletion, ResultStreamMetadata};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_transaction::TransactionState;
use andromeda_wal::Lsn;

pub(crate) fn completion_status_transaction_state(
    status: CompletionStatus,
) -> AndromedaResult<TransactionState> {
    match status {
        CompletionStatus::Committed => Ok(TransactionState::Committed),
        CompletionStatus::RolledBack => Ok(TransactionState::RolledBack),
        _ => Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            format!(
                "result stream completion requires terminal status; got {:?}",
                status
            ),
        )),
    }
}

pub(crate) fn validate_terminal_completion(
    metadata: ResultStreamMetadata,
    transaction_state: TransactionState,
    durable_lsn: Lsn,
    actual_row_count: u64,
) -> AndromedaResult<()> {
    if !matches!(
        transaction_state,
        TransactionState::Committed | TransactionState::RolledBack
    ) {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            "result stream completion requires a terminal transaction state",
        ));
    }

    if durable_lsn.is_zero() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            "result stream completion requires nonzero durable LSN evidence",
        ));
    }

    if transaction_state == TransactionState::RolledBack && actual_row_count != 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            "rolled-back result stream completion must report zero rows",
        ));
    }

    metadata.validate_completed_stream(actual_row_count)
}

pub(crate) fn validate_invocation_completion(
    completion: InvocationCompletion,
) -> AndromedaResult<()> {
    if completion.invocation_id.get() == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Execution,
            "invocation completion id must not be zero",
        ));
    }

    match completion.status {
        CompletionStatus::Committed => validate_committed_completion(completion),
        CompletionStatus::RolledBack => validate_rolled_back_completion(completion),
        CompletionStatus::FailedBeforeTransaction
        | CompletionStatus::Cancelled
        | CompletionStatus::Poisoned
        | CompletionStatus::PermissionDenied
        | CompletionStatus::ContractRejected
        | CompletionStatus::SystemUnavailable => validate_non_transactional_completion(completion),
    }
}

fn validate_committed_completion(completion: InvocationCompletion) -> AndromedaResult<()> {
    if completion.transaction_state != Some(TransactionState::Committed) {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            "committed completion requires committed transaction state",
        ));
    }
    if completion.rows_affected.is_none() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Execution,
            "committed completion requires rows affected metadata",
        ));
    }
    let Some(durable_lsn) = completion.durable_lsn else {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            "committed completion requires durable WAL LSN evidence",
        ));
    };
    if durable_lsn.is_zero() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            "committed completion requires nonzero durable WAL LSN evidence",
        ));
    }

    Ok(())
}

fn validate_rolled_back_completion(completion: InvocationCompletion) -> AndromedaResult<()> {
    if completion.transaction_state != Some(TransactionState::RolledBack) {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            "rolled-back completion requires rolled-back transaction state",
        ));
    }
    if completion.rows_affected != Some(0) {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Execution,
            "rolled-back completion must report zero rows affected",
        ));
    }
    let Some(durable_lsn) = completion.durable_lsn else {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            "rolled-back completion requires durable WAL LSN evidence",
        ));
    };
    if durable_lsn.is_zero() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            "rolled-back completion requires nonzero durable WAL LSN evidence",
        ));
    }

    Ok(())
}

fn validate_non_transactional_completion(completion: InvocationCompletion) -> AndromedaResult<()> {
    if completion.transaction_state.is_some()
        || completion.rows_affected.is_some()
        || completion.durable_lsn.is_some()
    {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Execution,
            "non-transactional completion must not carry transaction evidence",
        ));
    }

    Ok(())
}
