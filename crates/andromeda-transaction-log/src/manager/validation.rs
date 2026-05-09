use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::TransactionId;

use crate::{Lsn, TransactionLogStatus, TransactionStatusStore, TxWalReplayRecord};

pub(in crate::manager) fn validate_transaction_id(
    tx_id: TransactionId,
    message: &'static str,
) -> AndromedaResult<()> {
    if tx_id.get() == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            message,
        ));
    }
    Ok(())
}

pub(in crate::manager) fn validate_durable_flush_covers_record(
    durable_lsn: Lsn,
    record_lsn: Lsn,
    record_name: &'static str,
) -> AndromedaResult<()> {
    if durable_lsn < record_lsn {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            format!(
                "durable WAL flush ended before {record_name} LSN: durable={}, record={}",
                durable_lsn.get(),
                record_lsn.get()
            ),
        ));
    }
    Ok(())
}

pub(in crate::manager) fn validate_replay_lsn_progression(
    previous_record: &mut Option<TxWalReplayRecord>,
    record: &TxWalReplayRecord,
) -> AndromedaResult<()> {
    let record_lsn = record.replay_lsn();
    if let Some(previous_record) = previous_record.as_ref() {
        let previous_lsn = previous_record.replay_lsn();
        if record_lsn < previous_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                format!(
                    "transaction WAL replay LSN order regressed: previous={}, current={}",
                    previous_lsn.get(),
                    record_lsn.get()
                ),
            ));
        }

        if record_lsn == previous_lsn && record != previous_record {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                format!(
                    "transaction WAL replay LSN reused by distinct records: lsn={}",
                    record_lsn.get()
                ),
            ));
        }
    }

    *previous_record = Some(record.clone());
    Ok(())
}

pub(in crate::manager) fn validate_terminal_status_compatible<S: TransactionStatusStore>(
    status_table: &S,
    tx_id: TransactionId,
    replay_status: TransactionLogStatus,
) -> AndromedaResult<()> {
    match status_table.status(tx_id) {
        Some(existing) if existing == replay_status => Ok(()),
        Some(TransactionLogStatus::InFlight) => Ok(()),
        Some(_) => Err(conflicting_terminal_record()),
        None => Ok(()),
    }
}

pub(in crate::manager) fn conflicting_terminal_record() -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Transaction,
        "conflicting transaction terminal durability records",
    )
}
