use andromeda_core::{AndromedaResult, TransactionId};
use andromeda_storage::Lsn;
use andromeda_tx::TransactionManager;

use crate::RollbackCause;

pub(super) fn begin_runtime_transaction(
    transactions: &mut TransactionManager,
) -> AndromedaResult<TransactionId> {
    transactions.begin()
}

pub(super) fn route_rollback_cause(
    transactions: &mut TransactionManager,
    transaction_id: TransactionId,
    cause: RollbackCause,
) -> AndromedaResult<()> {
    match cause {
        RollbackCause::Direct => Ok(()),
        RollbackCause::BusinessFailure => transactions.fail(transaction_id),
        RollbackCause::Poison => transactions.poison(transaction_id),
    }
}

pub(super) fn mark_commit_visible_after_durable_wal(
    transactions: &mut TransactionManager,
    transaction_id: TransactionId,
    durable_lsn: Lsn,
) -> AndromedaResult<()> {
    transactions.request_commit(transaction_id)?;
    transactions.commit_durable(transaction_id, durable_lsn.get())?;
    transactions.dispose(transaction_id)
}

pub(super) fn mark_rollback_durable_after_wal(
    transactions: &mut TransactionManager,
    transaction_id: TransactionId,
    durable_lsn: Lsn,
) -> AndromedaResult<()> {
    transactions.request_rollback(transaction_id)?;
    transactions.rollback_durable(transaction_id, durable_lsn.get())?;
    transactions.dispose(transaction_id)
}
