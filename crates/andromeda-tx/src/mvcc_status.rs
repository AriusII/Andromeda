//! Transaction status tracking for MVCC visibility.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use std::collections::BTreeMap;

/// Status of a transaction in the execution lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionStatus {
    InFlight,
    Committed,
    RolledBack,
}

/// Registry of transaction statuses for visibility determination.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransactionStatusTable {
    statuses: BTreeMap<TransactionId, TransactionStatus>,
}

impl TransactionStatusTable {
    pub fn new() -> Self {
        Self {
            statuses: BTreeMap::new(),
        }
    }

    pub fn record(
        &mut self,
        transaction_id: TransactionId,
        status: TransactionStatus,
    ) -> AndromedaResult<()> {
        validate_transaction_id(transaction_id, "transaction status id must not be zero")?;
        self.statuses.insert(transaction_id, status);
        Ok(())
    }

    pub fn status(&self, transaction_id: TransactionId) -> Option<TransactionStatus> {
        self.statuses.get(&transaction_id).copied()
    }

    pub fn status_for_snapshot(
        &self,
        transaction_id: TransactionId,
        version_ts: u64,
        snapshot: &crate::mvcc_snapshot::Snapshot,
    ) -> TransactionStatus {
        self.status(transaction_id).unwrap_or_else(|| {
            if snapshot.is_transaction_active(transaction_id) || version_ts > snapshot.timestamp {
                TransactionStatus::InFlight
            } else {
                TransactionStatus::Committed
            }
        })
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
        let mut table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        assert!(table.record(tx_id, TransactionStatus::InFlight).is_ok());
        assert_eq!(table.status(tx_id), Some(TransactionStatus::InFlight));

        assert!(table.record(tx_id, TransactionStatus::Committed).is_ok());
        assert_eq!(table.status(tx_id), Some(TransactionStatus::Committed));
    }

    #[test]
    fn transaction_status_rejects_zero_id() {
        let mut table = TransactionStatusTable::new();
        let result = table.record(TransactionId::new(0), TransactionStatus::Committed);
        assert!(result.is_err());
    }
}
