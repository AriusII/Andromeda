//! Commit log boundary for WAL durability-before-visibility ordering.

use andromeda_error::AndromedaResult;
use andromeda_types::TransactionId;

use crate::Lsn;

use super::commit_log_entry::{CommitLog, CommitLogEntry, Timestamp, transaction_error};

/// Boundary coordinating the commit-log cache with WAL durability evidence.
pub struct CommitLogFacade {
    commit_log: CommitLog,
}

impl CommitLogFacade {
    pub fn new() -> Self {
        Self {
            commit_log: CommitLog::new(),
        }
    }

    pub fn record_commit(
        &self,
        tx_id: TransactionId,
        commit_lsn: Lsn,
        visible_ts: Timestamp,
    ) -> AndromedaResult<()> {
        let entry = CommitLogEntry::new(tx_id, commit_lsn, visible_ts)?;
        self.commit_log.record_commit(entry)
    }

    pub fn confirm_durable(&self, tx_id: TransactionId) -> AndromedaResult<()> {
        self.commit_log.confirm_durable(tx_id)
    }

    pub fn make_visible(&self, tx_id: TransactionId) -> AndromedaResult<()> {
        match self.commit_log.query_commit_status(tx_id)? {
            Some(entry) if entry.is_durable() => Ok(()),
            Some(_) => Err(transaction_error(format!(
                "cannot make visible: transaction {} is not durable yet",
                tx_id.get()
            ))),
            None => Err(transaction_error(format!(
                "commit log entry not found for transaction {}",
                tx_id.get()
            ))),
        }
    }

    pub fn query_status(&self, tx_id: TransactionId) -> AndromedaResult<Option<CommitLogEntry>> {
        self.commit_log.query_commit_status(tx_id)
    }

    pub fn cleanup_before_lsn(&self, before_lsn: Lsn) -> usize {
        self.commit_log.cleanup_entries(before_lsn)
    }
}

impl Default for CommitLogFacade {
    fn default() -> Self {
        Self::new()
    }
}
