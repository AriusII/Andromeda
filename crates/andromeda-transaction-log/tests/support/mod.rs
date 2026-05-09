use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_transaction_log::{Lsn, TransactionLogStatus, TransactionStatusStore};
use andromeda_types::TransactionId;
use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

pub(crate) type TransactionStatus = TransactionLogStatus;

#[derive(Debug, Default)]
pub(crate) struct TransactionStatusTable {
    statuses: Mutex<BTreeMap<TransactionId, TransactionStatus>>,
}

impl TransactionStatusTable {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn status(&self, transaction_id: TransactionId) -> Option<TransactionStatus> {
        self.lock_statuses().get(&transaction_id).copied()
    }

    fn record_terminal_from_durable_evidence(
        &self,
        transaction_id: TransactionId,
        status: TransactionStatus,
        record_lsn: Lsn,
        durable_lsn: Lsn,
    ) -> AndromedaResult<()> {
        validate_transaction_id(transaction_id)?;
        validate_terminal_lsn(record_lsn, durable_lsn)?;
        self.record_validated_terminal_status(transaction_id, status)
    }

    fn record_validated_terminal_status(
        &self,
        transaction_id: TransactionId,
        status: TransactionStatus,
    ) -> AndromedaResult<()> {
        if !status.is_terminal() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "validated replay status must be terminal",
            ));
        }

        let mut statuses = self.lock_statuses();
        match statuses.get(&transaction_id).copied() {
            Some(existing) if existing == status => Ok(()),
            Some(TransactionStatus::InFlight) | None => {
                statuses.insert(transaction_id, status);
                Ok(())
            },
            Some(_) => Err(conflicting_status()),
        }
    }

    fn lock_statuses(&self) -> MutexGuard<'_, BTreeMap<TransactionId, TransactionStatus>> {
        self.statuses
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl TransactionStatusStore for TransactionStatusTable {
    fn status(&self, tx_id: TransactionId) -> Option<TransactionStatus> {
        self.status(tx_id)
    }

    fn record_commit_from_durable_evidence(
        &self,
        tx_id: TransactionId,
        commit_lsn: Lsn,
        durable_lsn: Lsn,
    ) -> AndromedaResult<()> {
        self.record_terminal_from_durable_evidence(
            tx_id,
            TransactionStatus::Committed,
            commit_lsn,
            durable_lsn,
        )
    }

    fn record_rollback_from_durable_evidence(
        &self,
        tx_id: TransactionId,
        rollback_lsn: Lsn,
        durable_lsn: Lsn,
    ) -> AndromedaResult<()> {
        self.record_terminal_from_durable_evidence(
            tx_id,
            TransactionStatus::RolledBack,
            rollback_lsn,
            durable_lsn,
        )
    }

    fn restore_terminal_from_validated_replay(
        &self,
        tx_id: TransactionId,
        status: TransactionStatus,
    ) -> AndromedaResult<()> {
        self.record_validated_terminal_status(tx_id, status)
    }
}

fn validate_transaction_id(transaction_id: TransactionId) -> AndromedaResult<()> {
    if transaction_id.get() == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            "transaction status id must not be zero",
        ));
    }
    Ok(())
}

fn validate_terminal_lsn(record_lsn: Lsn, durable_lsn: Lsn) -> AndromedaResult<()> {
    if record_lsn.get() == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            "terminal transaction status record LSN must not be zero",
        ));
    }

    if durable_lsn < record_lsn {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            format!(
                "terminal transaction status durable LSN must cover record LSN: durable={}, record={}",
                durable_lsn.get(),
                record_lsn.get()
            ),
        ));
    }

    Ok(())
}

fn conflicting_status() -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Transaction,
        "conflicting transaction terminal status",
    )
}
