//! Snapshot-based consistency view for MVCC.

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, TransactionId,
};

/// MVCC isolation policy for determining visibility rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MvccIsolationPolicy {
    ReadCommitted,
    RepeatableRead,
}

/// Point-in-time consistent snapshot for version visibility.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub timestamp: u64,
    pub catalog_version: Option<CatalogVersion>,
    pub isolation_policy: MvccIsolationPolicy,
    pub transaction_id: Option<TransactionId>,
    pub active_tx_ids: Vec<TransactionId>,
}

impl Snapshot {
    pub fn new(timestamp: u64) -> Self {
        Self {
            timestamp,
            catalog_version: None,
            isolation_policy: MvccIsolationPolicy::ReadCommitted,
            transaction_id: None,
            active_tx_ids: Vec::new(),
        }
    }

    pub fn with_context(
        timestamp: u64,
        catalog_version: CatalogVersion,
        isolation_policy: MvccIsolationPolicy,
        transaction_id: Option<TransactionId>,
        active_tx_ids: impl IntoIterator<Item = TransactionId>,
    ) -> AndromedaResult<Self> {
        let mut active_tx_ids: Vec<_> = active_tx_ids.into_iter().collect();
        active_tx_ids.sort_unstable();
        active_tx_ids.dedup();

        let snapshot = Self {
            timestamp,
            catalog_version: Some(catalog_version),
            isolation_policy,
            transaction_id,
            active_tx_ids,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn is_current_transaction(&self, transaction_id: TransactionId) -> bool {
        self.transaction_id == Some(transaction_id)
    }

    pub fn is_transaction_active(&self, transaction_id: TransactionId) -> bool {
        self.active_tx_ids.binary_search(&transaction_id).is_ok()
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.timestamp == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "snapshot timestamp must not be zero",
            ));
        }

        if matches!(self.catalog_version, Some(catalog_version) if catalog_version.get() == 0) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "snapshot catalog version must not be zero",
            ));
        }

        if matches!(self.transaction_id, Some(transaction_id) if transaction_id.get() == 0) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "snapshot transaction id must not be zero",
            ));
        }

        let mut previous = None;
        for transaction_id in &self.active_tx_ids {
            if transaction_id.get() == 0 {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Transaction,
                    "snapshot active transaction id must not be zero",
                ));
            }

            if previous.is_some_and(|previous| previous >= *transaction_id) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Transaction,
                    "snapshot active transaction ids must be sorted and unique",
                ));
            }

            previous = Some(*transaction_id);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_context_normalizes_active_transactions() {
        let snapshot = Snapshot::with_context(
            50,
            CatalogVersion::new(7),
            MvccIsolationPolicy::RepeatableRead,
            Some(TransactionId::new(1)),
            [
                TransactionId::new(9),
                TransactionId::new(3),
                TransactionId::new(9),
            ],
        )
        .unwrap();

        assert_eq!(
            snapshot.active_tx_ids,
            vec![TransactionId::new(3), TransactionId::new(9)]
        );
    }
}
