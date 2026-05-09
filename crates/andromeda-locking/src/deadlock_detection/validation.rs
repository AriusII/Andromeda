use andromeda_error::{AndromedaError, AndromedaErrorKind};
use andromeda_types::TransactionId;

use super::{DeadlockError, DeadlockResult};

pub(super) fn validate_transaction_id(tx_id: TransactionId) -> DeadlockResult<()> {
    if tx_id.get() == 0 {
        return Err(deadlock_error("deadlock transaction id must not be zero"));
    }

    Ok(())
}

pub(super) fn validate_start_order(start_order: u64) -> DeadlockResult<()> {
    if start_order == 0 {
        return Err(deadlock_error(
            "deadlock transaction start order must not be zero",
        ));
    }

    Ok(())
}

pub(super) fn validate_not_self_edge(
    waiting_tx: TransactionId,
    blocking_tx: TransactionId,
) -> DeadlockResult<()> {
    if waiting_tx == blocking_tx {
        return Err(deadlock_error("deadlock wait-for self edge is invalid"));
    }

    Ok(())
}

pub(super) fn validate_cycle_participants(
    cycle_participants: &[TransactionId],
) -> DeadlockResult<()> {
    if cycle_participants.len() < 2 {
        return Err(deadlock_error(
            "deadlock cycle must contain at least two transactions",
        ));
    }

    for tx_id in cycle_participants {
        validate_transaction_id(*tx_id)?;
    }

    Ok(())
}

pub(super) fn deadlock_error(message: &'static str) -> DeadlockError {
    AndromedaError::new(AndromedaErrorKind::Transaction, message)
}
