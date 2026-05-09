use andromeda_error::AndromedaResult;
use andromeda_types::TransactionId;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;

use crate::Lsn;

use super::{CommitLogEntry, transaction_error};

/// Reconstructable commit log cache keyed by transaction id.
pub struct CommitLog {
    entries: DashMap<TransactionId, CommitLogEntry>,
}

impl CommitLog {
    pub fn new() -> Self {
        CommitLog {
            entries: DashMap::new(),
        }
    }

    pub fn record_commit(&self, mut entry: CommitLogEntry) -> AndromedaResult<()> {
        entry.wal_durability_confirmed = false;

        match self.entries.entry(entry.tx_id) {
            Entry::Occupied(occupied) => Err(transaction_error(format!(
                "commit log entry already exists for transaction {}",
                occupied.key().get()
            ))),
            Entry::Vacant(vacant) => {
                vacant.insert(entry);
                Ok(())
            },
        }
    }

    pub fn confirm_durable(&self, tx_id: TransactionId) -> AndromedaResult<()> {
        match self.entries.get_mut(&tx_id) {
            Some(mut entry) => {
                entry.mark_durable();
                Ok(())
            },
            None => Err(transaction_error(format!(
                "commit log entry not found for transaction {}",
                tx_id.get()
            ))),
        }
    }

    pub fn query_commit_status(
        &self,
        tx_id: TransactionId,
    ) -> AndromedaResult<Option<CommitLogEntry>> {
        Ok(self.entries.get(&tx_id).map(|ref_multi| ref_multi.clone()))
    }

    pub fn cleanup_entries(&self, before_lsn: Lsn) -> usize {
        let mut count = 0;
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

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub fn all_tx_ids(&self) -> Vec<TransactionId> {
        self.entries
            .iter()
            .map(|ref_multi| *ref_multi.key())
            .collect()
    }

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
