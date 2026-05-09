//! Transaction status tracking for MVCC visibility.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use andromeda_transaction_log::Lsn;
use andromeda_types::TransactionId;
use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

/// Status of a transaction in the execution lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionStatus {
    InFlight,
    Committed,
    RolledBack,
}

impl TransactionStatus {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Committed | Self::RolledBack)
    }
}

/// Registry of transaction statuses for visibility determination.
#[derive(Debug)]
pub struct TransactionStatusTable {
    statuses: Mutex<BTreeMap<TransactionId, TransactionStatus>>,
}

impl TransactionStatusTable {
    pub fn new() -> Self {
        Self {
            statuses: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn record(
        &self,
        transaction_id: TransactionId,
        status: TransactionStatus,
    ) -> AndromedaResult<()> {
        validate_transaction_id(transaction_id, "transaction status id must not be zero")?;
        if status.is_terminal() {
            return Err(terminal_status_requires_durable_wal());
        }

        let mut statuses = self.lock_statuses();
        match statuses.get(&transaction_id).copied() {
            Some(existing) if existing.is_terminal() => {
                return Err(terminal_status_conflict());
            },
            _ => {
                statuses.insert(transaction_id, status);
            },
        }
        Ok(())
    }

    pub fn record_in_flight(&self, transaction_id: TransactionId) -> AndromedaResult<()> {
        self.record(transaction_id, TransactionStatus::InFlight)
    }

    pub fn status(&self, transaction_id: TransactionId) -> Option<TransactionStatus> {
        let statuses = self.lock_statuses();
        statuses.get(&transaction_id).copied()
    }

    /// Kept for source compatibility, but terminal publication now requires
    /// durable WAL evidence through the transaction commit-log boundary.
    pub fn set_committed(&self, transaction_id: TransactionId) -> AndromedaResult<()> {
        validate_transaction_id(transaction_id, "transaction status id must not be zero")?;
        Err(terminal_status_requires_durable_wal())
    }

    pub fn record_committed_after_durable_wal(
        &self,
        transaction_id: TransactionId,
        commit_lsn: Lsn,
        durable_lsn: Lsn,
    ) -> AndromedaResult<()> {
        self.record_terminal_from_durable_evidence(
            transaction_id,
            TransactionStatus::Committed,
            commit_lsn,
            durable_lsn,
        )
    }

    pub fn record_rolled_back_after_durable_wal(
        &self,
        transaction_id: TransactionId,
        rollback_lsn: Lsn,
        durable_lsn: Lsn,
    ) -> AndromedaResult<()> {
        self.record_terminal_from_durable_evidence(
            transaction_id,
            TransactionStatus::RolledBack,
            rollback_lsn,
            durable_lsn,
        )
    }

    #[doc(hidden)]
    pub fn record_commit_from_durable_evidence(
        &self,
        transaction_id: TransactionId,
        commit_lsn: Lsn,
        durable_lsn: Lsn,
    ) -> AndromedaResult<()> {
        self.record_committed_after_durable_wal(transaction_id, commit_lsn, durable_lsn)
    }

    #[doc(hidden)]
    pub fn record_rollback_from_durable_evidence(
        &self,
        transaction_id: TransactionId,
        rollback_lsn: Lsn,
        durable_lsn: Lsn,
    ) -> AndromedaResult<()> {
        self.record_rolled_back_after_durable_wal(transaction_id, rollback_lsn, durable_lsn)
    }

    #[doc(hidden)]
    pub fn restore_terminal_from_validated_replay(
        &self,
        transaction_id: TransactionId,
        status: TransactionStatus,
    ) -> AndromedaResult<()> {
        validate_transaction_id(transaction_id, "transaction status id must not be zero")?;
        if !status.is_terminal() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "validated replay status must be terminal",
            ));
        }
        self.record_validated_terminal_status(transaction_id, status)
    }

    /// Get the status of a transaction.
    pub fn get_status(&self, transaction_id: TransactionId) -> Option<TransactionStatus> {
        self.status(transaction_id)
    }

    /// Returns `true` only when the manager has explicitly recorded the
    /// transaction as `Committed`. Used by snapshot validation and tests
    /// to assert the durable-commit doctrine.
    pub fn is_durable_committed(&self, transaction_id: TransactionId) -> bool {
        matches!(
            self.status(transaction_id),
            Some(TransactionStatus::Committed)
        )
    }

    /// Returns `true` if the transaction is recorded as `InFlight`.
    pub fn is_in_flight(&self, transaction_id: TransactionId) -> bool {
        matches!(
            self.status(transaction_id),
            Some(TransactionStatus::InFlight)
        )
    }

    /// Resolve the visibility status of `transaction_id` for an MVCC version
    /// authored at `version_ts`.
    ///
    /// **V0 doctrine:** a transaction is *only* considered `Committed` when
    /// the manager has durably recorded that outcome (see
    /// `andromeda-tx` durable commit boundary). If no entry exists,
    /// the writer is treated as `InFlight` — i.e. invisible to any other
    /// snapshot — even if its `version_ts` precedes `snapshot.timestamp`.
    ///
    /// This eliminates the previous heuristic "no record + version older
    /// than snapshot ⇒ assume committed", which violated the rule
    /// `visible commit ≡ durable WAL`. Callers that need to observe a
    /// transaction's writes must therefore arrange for that transaction's
    /// durable commit to be mirrored into this table before issuing reads.
    ///
    /// `version_ts` and `snapshot` are kept in the signature to preserve
    /// the call shape and to allow future refinements (e.g. distinguishing
    /// "writer started after snapshot" from "writer is still in flight"
    /// for diagnostics) without another breaking change.
    pub fn status_for_snapshot(
        &self,
        transaction_id: TransactionId,
        _version_ts: u64,
        _snapshot: &crate::Snapshot,
    ) -> TransactionStatus {
        self.status(transaction_id)
            .unwrap_or(TransactionStatus::InFlight)
    }

    fn record_terminal_from_durable_evidence(
        &self,
        transaction_id: TransactionId,
        status: TransactionStatus,
        terminal_record_lsn: Lsn,
        durable_lsn: Lsn,
    ) -> AndromedaResult<()> {
        validate_terminal_status(status)?;
        validate_terminal_lsn_evidence(terminal_record_lsn, durable_lsn)?;
        self.record_validated_terminal_status(transaction_id, status)
    }

    fn record_validated_terminal_status(
        &self,
        transaction_id: TransactionId,
        status: TransactionStatus,
    ) -> AndromedaResult<()> {
        validate_terminal_status(status)?;
        let mut statuses = self.lock_statuses();
        match statuses.get(&transaction_id).copied() {
            Some(existing) if existing == status => Ok(()),
            Some(TransactionStatus::InFlight) | None => {
                statuses.insert(transaction_id, status);
                Ok(())
            },
            Some(_) => Err(terminal_status_conflict()),
        }
    }

    fn lock_statuses(&self) -> MutexGuard<'_, BTreeMap<TransactionId, TransactionStatus>> {
        self.statuses
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Default for TransactionStatusTable {
    fn default() -> Self {
        Self::new()
    }
}

fn validate_transaction_id(
    transaction_id: TransactionId,
    message: &'static str,
) -> AndromedaResult<()> {
    if transaction_id.get() == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            message,
        ));
    }

    Ok(())
}

fn validate_terminal_status(status: TransactionStatus) -> AndromedaResult<()> {
    if status.is_terminal() {
        Ok(())
    } else {
        Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            "terminal transaction status expected",
        ))
    }
}

fn validate_terminal_lsn_evidence(
    terminal_record_lsn: Lsn,
    durable_lsn: Lsn,
) -> AndromedaResult<()> {
    if terminal_record_lsn.get() == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            "terminal transaction status record LSN must not be zero",
        ));
    }

    if durable_lsn < terminal_record_lsn {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            format!(
                "terminal transaction status durable LSN must cover record LSN: durable={}, record={}",
                durable_lsn.get(),
                terminal_record_lsn.get()
            ),
        ));
    }

    Ok(())
}

fn terminal_status_requires_durable_wal() -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Transaction,
        "terminal transaction status requires durable WAL evidence",
    )
}

fn terminal_status_conflict() -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Transaction,
        "conflicting transaction terminal status",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_status_records_non_terminal_and_retrieves() {
        let table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        assert!(table.record(tx_id, TransactionStatus::InFlight).is_ok());
        assert_eq!(table.status(tx_id), Some(TransactionStatus::InFlight));
    }

    #[test]
    fn transaction_status_rejects_public_terminal_recording() {
        let table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(2);

        let commit = table.record(tx_id, TransactionStatus::Committed);
        assert!(commit.is_err());
        assert_eq!(table.status(tx_id), None);

        let rollback = table.record(tx_id, TransactionStatus::RolledBack);
        assert!(rollback.is_err());
        assert_eq!(table.status(tx_id), None);
    }

    #[test]
    fn transaction_status_rejects_zero_id() {
        let table = TransactionStatusTable::new();
        let result = table.record(TransactionId::new(0), TransactionStatus::InFlight);
        assert!(result.is_err());
    }

    #[test]
    fn transaction_status_set_committed_rejects_without_evidence() {
        let table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(42);

        assert!(table.set_committed(tx_id).is_err());
        assert_eq!(table.status(tx_id), None);
    }

    #[test]
    fn transaction_status_internal_terminal_recording_requires_durable_coverage() {
        let table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(43);

        assert!(
            table
                .record_commit_from_durable_evidence(tx_id, Lsn::new(7), Lsn::new(6))
                .is_err()
        );
        assert_eq!(table.status(tx_id), None);

        assert!(
            table
                .record_commit_from_durable_evidence(tx_id, Lsn::new(7), Lsn::new(7),)
                .is_ok()
        );
        assert_eq!(table.status(tx_id), Some(TransactionStatus::Committed));
    }

    #[test]
    fn transaction_status_rejects_terminal_downgrade_to_in_flight() {
        let table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(44);

        assert!(
            table
                .record_commit_from_durable_evidence(tx_id, Lsn::new(8), Lsn::new(8))
                .is_ok()
        );

        let error = table
            .record(tx_id, TransactionStatus::InFlight)
            .expect_err("durable terminal status must not be downgraded");

        assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
        assert_eq!(table.status(tx_id), Some(TransactionStatus::Committed));
    }
}
