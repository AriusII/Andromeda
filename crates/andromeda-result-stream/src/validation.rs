use crate::{CompletionStatus, InvocationCompletion, ResultStreamMetadata};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_transaction::TransactionState;
use andromeda_wal::Lsn;

pub(crate) fn completion_status_transaction_state(
    status: CompletionStatus,
) -> AndromedaResult<TransactionState> {
    ensure_transactional_completion_status(status)?;

    Ok(match status {
        CompletionStatus::Committed => TransactionState::Committed,
        CompletionStatus::RolledBack => TransactionState::RolledBack,
        _ => unreachable!("terminal result stream status already checked"),
    })
}

pub(crate) fn ensure_transactional_completion_status(
    status: CompletionStatus,
) -> AndromedaResult<()> {
    if status.is_transactional_terminal() {
        return Ok(());
    }

    Err(AndromedaError::new(
        AndromedaErrorKind::Transaction,
        format!(
            "result stream completion requires terminal status; got {:?}",
            status
        ),
    ))
}

pub(crate) fn ensure_nonzero_durable_lsn(durable_lsn: Lsn) -> AndromedaResult<()> {
    if durable_lsn.is_zero() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            "result stream completion requires nonzero durable LSN evidence",
        ));
    }

    Ok(())
}

pub(crate) fn ensure_rollback_reports_zero_rows(
    transaction_state: TransactionState,
    actual_row_count: u64,
) -> AndromedaResult<()> {
    if transaction_state == TransactionState::RolledBack && actual_row_count != 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            "rolled-back result stream completion must report zero rows",
        ));
    }

    Ok(())
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

    ensure_nonzero_durable_lsn(durable_lsn)?;
    ensure_rollback_reports_zero_rows(transaction_state, actual_row_count)?;

    metadata.validate_completed_stream(actual_row_count)
}

/// Validate that the transaction evidence is consistent with a terminal
/// commitment: the transaction must be in a terminal state, the durable LSN
/// must be non-zero (WAL is flushed), and a rolled-back transaction must
/// report zero DB mutations.
///
/// This is the correct check to call on the hot commit path in
/// `execute_after_admission()` because it validates WAL and MVCC evidence
/// **without** cross-comparing DB mutation rows (`rows_affected`) against
/// result-stream row count (`row_count_exact`). Those two fields represent
/// intentionally different quantities:
///
/// - `db_rows_affected`: total rows mutated in the database (≥ result rows
///   for denormalised writes, 0 for read-only procedures).
/// - `result_metadata.row_count_exact`: the number of rows in the result
///   stream returned to the caller (varies independently of DB mutations).
///
/// Use `validate_terminal_completion` (which calls `validate_completed_stream`)
/// only when the caller has a genuine result-stream row count to cross-check.
pub(crate) fn validate_terminal_evidence(
    transaction_state: TransactionState,
    durable_lsn: Lsn,
    db_rows_affected: u64,
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

    ensure_nonzero_durable_lsn(durable_lsn)?;
    ensure_rollback_reports_zero_rows(transaction_state, db_rows_affected)?;

    Ok(())
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
