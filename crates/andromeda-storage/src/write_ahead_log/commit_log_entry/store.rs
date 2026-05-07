use andromeda_core::{AndromedaResult, TransactionId};
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;

use crate::Lsn;

use super::{CommitLogEntry, transaction_error};

/// Commit log: in-memory snapshot cache with persistence support.
///
/// Maintains a fast lookup table of committed transactions with their
/// WAL durability evidence. Primary storage is the WAL; this structure
/// is a snapshot cache rebuilt on recovery.
pub struct CommitLog {
    /// Fast lookup: TxId → CommitLogEntry
    entries: DashMap<TransactionId, CommitLogEntry>,
}

impl CommitLog {
    /// Create a new empty commit log.
    pub fn new() -> Self {
        CommitLog {
            entries: DashMap::new(),
        }
    }

    /// Record a commit: log entry and (atomically) mark to WAL.
    ///
    /// # Invariant
    ///
    /// Entry is stored with `wal_durability_confirmed = false` initially.
    /// Caller MUST call `confirm_durable()` after WAL flush completes.
    ///
    /// # Arguments
    ///
    /// * `entry` - Commit log entry to record
    ///
    /// # Errors
    ///
    /// Returns error if entry already exists for this transaction ID.
    pub fn record_commit(&self, mut entry: CommitLogEntry) -> AndromedaResult<()> {
        // Ensure entry is created with durability NOT yet confirmed
        entry.wal_durability_confirmed = false;

        match self.entries.entry(entry.tx_id) {
            Entry::Occupied(occupied) => Err(transaction_error(format!(
                "commit log entry already exists for transaction {}",
                occupied.key().get()
            ))),
            Entry::Vacant(vacant) => {
                vacant.insert(entry);
                Ok(())
            }
        }
    }

    /// Confirm durability for a commit entry.
    ///
    /// This MUST be called after WAL flush completes to atomically
    /// flip the `wal_durability_confirmed` flag.
    ///
    /// # Invariant
    ///
    /// After this call returns, `query_commit_status()` will return
    /// an entry with `is_durable() == true`.
    ///
    /// # Arguments
    ///
    /// * `tx_id` - Transaction to mark as durable
    ///
    /// # Errors
    ///
    /// Returns error if no entry exists for this transaction ID.
    pub fn confirm_durable(&self, tx_id: TransactionId) -> AndromedaResult<()> {
        match self.entries.get_mut(&tx_id) {
            Some(mut entry) => {
                entry.mark_durable();
                Ok(())
            }
            None => Err(transaction_error(format!(
                "commit log entry not found for transaction {}",
                tx_id.get()
            ))),
        }
    }

    /// Query commit status for a transaction.
    ///
    /// # Returns
    ///
    /// `Some(entry)` if transaction has a commit log entry, `None` otherwise.
    ///
    /// # Errors
    ///
    /// No errors: always returns successfully (entry may not exist).
    pub fn query_commit_status(
        &self,
        tx_id: TransactionId,
    ) -> AndromedaResult<Option<CommitLogEntry>> {
        Ok(self.entries.get(&tx_id).map(|ref_multi| ref_multi.clone()))
    }

    /// Clean up entries before a given LSN (for GC after backup).
    ///
    /// Removes all entries with `commit_lsn < before_lsn` that have
    /// durability confirmed. This is safe because:
    /// 1. Visibility has already been registered with TransactionStatusTable
    /// 2. Old commits are not needed for future WAL recovery
    ///
    /// # Returns
    ///
    /// Number of entries cleaned up.
    pub fn cleanup_entries(&self, before_lsn: Lsn) -> usize {
        let mut count = 0;

        // Collect IDs to remove (to avoid holding lock during iteration)
        let to_remove: Vec<TransactionId> = self
            .entries
            .iter()
            .filter(|ref_multi| {
                let entry = ref_multi.value();
                entry.commit_lsn < before_lsn && entry.is_durable()
            })
            .map(|ref_multi| *ref_multi.key())
            .collect();

        for tx_id in to_remove {
            if self.entries.remove(&tx_id).is_some() {
                count += 1;
            }
        }

        count
    }

    /// Get count of entries currently in the log.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Get all transaction IDs in the log.
    #[cfg(test)]
    pub fn all_tx_ids(&self) -> Vec<TransactionId> {
        self.entries
            .iter()
            .map(|ref_multi| *ref_multi.key())
            .collect()
    }

    /// Clear all entries.
    #[cfg(test)]
    pub fn clear(&self) {
        self.entries.clear();
    }
}

impl Default for CommitLog {
    fn default() -> Self {
        Self::new()
    }
}
