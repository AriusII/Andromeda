use std::collections::BTreeMap;

use andromeda_types::TransactionId;

use super::{DeadlockResult, validate_start_order, validate_transaction_id};

/// Storage-agnostic transaction ordering metadata used for victim selection.
///
/// `start_order` is a monotonic transaction-begin order supplied by the
/// transaction layer. Larger values are treated as younger transactions. Equal
/// values are legal so import/replay callers can model coarse ordering; ties are
/// broken by the greatest [`TransactionId`] to keep victim choice deterministic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeadlockTransactionMetadata {
    pub tx_id: TransactionId,
    pub start_order: u64,
}

impl DeadlockTransactionMetadata {
    pub fn new(tx_id: TransactionId, start_order: u64) -> DeadlockResult<Self> {
        validate_transaction_id(tx_id)?;
        validate_start_order(start_order)?;

        Ok(Self { tx_id, start_order })
    }

    fn validate(self) -> DeadlockResult<()> {
        validate_transaction_id(self.tx_id)?;
        validate_start_order(self.start_order)?;
        Ok(())
    }
}

/// Deterministic, storage-agnostic table for transaction ordering metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeadlockTransactionMetadataTable {
    entries: BTreeMap<TransactionId, DeadlockTransactionMetadata>,
}

impl DeadlockTransactionMetadataTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or replace ordering metadata for one transaction.
    ///
    /// Returns the previously registered metadata when an entry is replaced.
    pub fn register(
        &mut self,
        metadata: DeadlockTransactionMetadata,
    ) -> DeadlockResult<Option<DeadlockTransactionMetadata>> {
        metadata.validate()?;
        Ok(self.entries.insert(metadata.tx_id, metadata))
    }

    /// Register or replace ordering metadata by transaction id and start order.
    pub fn register_start_order(
        &mut self,
        tx_id: TransactionId,
        start_order: u64,
    ) -> DeadlockResult<Option<DeadlockTransactionMetadata>> {
        self.register(DeadlockTransactionMetadata::new(tx_id, start_order)?)
    }

    pub fn metadata_for(
        &self,
        tx_id: TransactionId,
    ) -> DeadlockResult<Option<DeadlockTransactionMetadata>> {
        validate_transaction_id(tx_id)?;
        Ok(self.entries.get(&tx_id).copied())
    }

    pub fn contains(&self, tx_id: TransactionId) -> DeadlockResult<bool> {
        validate_transaction_id(tx_id)?;
        Ok(self.entries.contains_key(&tx_id))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(super) fn has_metadata_for_all(&self, tx_ids: &[TransactionId]) -> DeadlockResult<bool> {
        for tx_id in tx_ids {
            validate_transaction_id(*tx_id)?;
            if !self.entries.contains_key(tx_id) {
                return Ok(false);
            }
        }

        Ok(true)
    }
}
