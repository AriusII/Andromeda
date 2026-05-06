//! Transaction status tracking for MVCC visibility.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

/// Status of a transaction in the execution lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionStatus {
    InFlight,
    Committed,
    RolledBack,
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
        let mut statuses = self.lock_statuses();
        statuses.insert(transaction_id, status);
        Ok(())
    }

    pub fn status(&self, transaction_id: TransactionId) -> Option<TransactionStatus> {
        let statuses = self.lock_statuses();
        statuses.get(&transaction_id).copied()
    }

    /// Set a transaction as committed (idempotent operation).
    pub fn set_committed(&self, transaction_id: TransactionId) -> AndromedaResult<()> {
        validate_transaction_id(transaction_id, "transaction status id must not be zero")?;
        let mut statuses = self.lock_statuses();
        statuses.insert(transaction_id, TransactionStatus::Committed);
        Ok(())
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
    /// [`crate::TransactionManager::commit_durable`]). If no entry exists,
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
        _snapshot: &crate::mvcc_snapshot::Snapshot,
    ) -> TransactionStatus {
        self.status(transaction_id)
            .unwrap_or(TransactionStatus::InFlight)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_status_records_and_retrieves() {
        let table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        assert!(table.record(tx_id, TransactionStatus::InFlight).is_ok());
        assert_eq!(table.status(tx_id), Some(TransactionStatus::InFlight));

        assert!(table.record(tx_id, TransactionStatus::Committed).is_ok());
        assert_eq!(table.status(tx_id), Some(TransactionStatus::Committed));
    }

    #[test]
    fn transaction_status_rejects_zero_id() {
        let table = TransactionStatusTable::new();
        let result = table.record(TransactionId::new(0), TransactionStatus::Committed);
        assert!(result.is_err());
    }

    #[test]
    fn transaction_status_set_committed() {
        let table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(42);

        assert!(table.set_committed(tx_id).is_ok());
        assert_eq!(table.status(tx_id), Some(TransactionStatus::Committed));
    }
}
