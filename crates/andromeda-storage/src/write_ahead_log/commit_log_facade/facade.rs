use andromeda_core::{AndromedaResult, TransactionId};
use std::sync::Arc;

use crate::Lsn;

use super::super::commit_log_entry::{CommitLog, CommitLogEntry, Timestamp, transaction_error};

/// Facade coordinating CommitLog cache with WAL persistence.
///
/// # Design Pattern
///
/// This facade implements the **Orchestrator** pattern:
/// - **Input:** Transaction commit events (tx_id, commit_lsn, visible_timestamp)
/// - **Process:** Create entry, emit WAL record, cache, confirm durability, publish visibility
/// - **Output:** AndromedaResult indicating success or error
///
/// # Usage Flow
///
/// ```ignore
/// // 1. Record commit after assigning the WAL commit LSN
/// facade.record_commit(tx_id, commit_lsn, visible_ts)?;
///
/// // 2. After WAL flush completes
/// facade.confirm_durable(tx_id)?;
///
/// // 3. When ready to publish visibility
/// facade.make_visible(tx_id)?;
///
/// // 4. Optionally query status
/// if let Some(entry) = facade.query_status(tx_id)? {
///     assert!(entry.is_durable());
/// }
/// ```
pub struct CommitLogFacade {
    /// In-memory commit log cache (fast lookup, atomic operations)
    commit_log: Arc<CommitLog>,
}

impl CommitLogFacade {
    /// Create a new CommitLogFacade with an empty commit log cache.
    pub fn new() -> Self {
        CommitLogFacade {
            commit_log: Arc::new(CommitLog::new()),
        }
    }

    /// Create a CommitLogFacade with a pre-populated commit log.
    ///
    /// Used during recovery to restore state from persisted logs.
    pub fn with_commit_log(commit_log: Arc<CommitLog>) -> Self {
        CommitLogFacade { commit_log }
    }

    /// Record a transaction commit: create entry and prepare for WAL integration.
    ///
    /// This method performs the following steps:
    /// 1. Create CommitLogEntry with tx_id, commit_lsn, and visible_timestamp
    /// 2. Store entry in cache with `is_durable() == false`
    /// 3. Return success (caller must write WAL record)
    ///
    /// # Invariant Enforcement
    ///
    /// - Entry is created **after** WAL LSN is assigned (LSN provided by caller)
    /// - Entry starts with `wal_durability_confirmed == false`
    /// - Caller MUST invoke `confirm_durable(tx_id)` after WAL flush
    ///
    /// # Arguments
    ///
    /// * `tx_id` - Transaction identifier (must be non-zero)
    /// * `commit_lsn` - LSN of the WAL commit record
    /// * `visible_ts` - Logical timestamp for visibility (must be non-zero)
    ///
    /// # Returns
    ///
    /// `Ok(())` if entry created and cached successfully.
    /// `Err` if tx_id is invalid or entry already exists.
    ///
    /// # Errors
    ///
    /// - `AndromedaErrorKind::Transaction` if tx_id is zero
    /// - `AndromedaErrorKind::Transaction` if visible_ts is zero
    /// - `AndromedaErrorKind::Transaction` if entry already exists for tx_id
    pub fn record_commit(
        &self,
        tx_id: TransactionId,
        commit_lsn: Lsn,
        visible_ts: Timestamp,
    ) -> AndromedaResult<()> {
        let entry = CommitLogEntry::new(tx_id, commit_lsn, visible_ts)?;
        self.commit_log.record_commit(entry)
    }

    /// Confirm that the WAL record for a commit is durable (flushed to disk).
    ///
    /// This method atomically sets the `is_durable()` flag to true.
    ///
    /// # Invariant Enforcement
    ///
    /// - Called **after** WAL flush operation completes
    /// - Atomically flips `wal_durability_confirmed` from false → true
    /// - Never reverted back to false
    /// - Must be called before `make_visible()` is invoked
    ///
    /// # Arguments
    ///
    /// * `tx_id` - Transaction to mark as durable
    ///
    /// # Returns
    ///
    /// `Ok(())` if durability confirmed.
    /// `Err` if tx_id not found in commit log.
    ///
    /// # Errors
    ///
    /// - `AndromedaErrorKind::Transaction` if no entry exists for tx_id
    pub fn confirm_durable(&self, tx_id: TransactionId) -> AndromedaResult<()> {
        self.commit_log.confirm_durable(tx_id)
    }

    /// Publish transaction as visible to all readers.
    ///
    /// This method marks the transaction as ready for snapshot visibility.
    /// Callers should update TransactionStatusTable after this call returns.
    ///
    /// # Invariant Enforcement
    ///
    /// - **Precondition:** `confirm_durable(tx_id)` must have completed
    /// - **Postcondition:** Entry is ready for visibility
    /// - Ensures "visible commit ≡ durable WAL" doctrine
    ///
    /// # Arguments
    ///
    /// * `tx_id` - Transaction to make visible
    ///
    /// # Returns
    ///
    /// `Ok(())` if visibility published.
    /// `Err` if tx_id not found OR entry is not durable.
    ///
    /// # Errors
    ///
    /// - `AndromedaErrorKind::Transaction` if no entry exists for tx_id
    /// - `AndromedaErrorKind::Transaction` if entry is not durable (precondition violation)
    pub fn make_visible(&self, tx_id: TransactionId) -> AndromedaResult<()> {
        match self.commit_log.query_commit_status(tx_id)? {
            Some(entry) => {
                if !entry.is_durable() {
                    return Err(transaction_error(format!(
                        "cannot make visible: transaction {} is not durable yet",
                        tx_id.get()
                    )));
                }
                Ok(())
            }
            None => Err(transaction_error(format!(
                "commit log entry not found for transaction {}",
                tx_id.get()
            ))),
        }
    }

    /// Query the commit status for a transaction.
    ///
    /// Returns the commit log entry if it exists, allowing caller to inspect:
    /// - `is_durable()`: whether WAL flush has confirmed
    /// - `commit_lsn()`: LSN of the commit record
    /// - `visible_timestamp()`: transaction's visibility timestamp
    ///
    /// # Arguments
    ///
    /// * `tx_id` - Transaction to query
    ///
    /// # Returns
    ///
    /// `Ok(Some(entry))` if transaction has a commit log entry.
    /// `Ok(None)` if transaction not found (never committed).
    ///
    /// # Errors
    ///
    /// Generally does not error; None indicates absence.
    pub fn query_status(&self, tx_id: TransactionId) -> AndromedaResult<Option<CommitLogEntry>> {
        self.commit_log.query_commit_status(tx_id)
    }

    /// Clean up entries that are persisted and no longer needed.
    ///
    /// Removes entries from the in-memory cache where:
    /// - Entry's commit_lsn < before_lsn
    /// - Entry is durable (persisted to disk)
    ///
    /// # Arguments
    ///
    /// * `before_lsn` - Entries with commit_lsn < before_lsn are candidates for removal
    ///
    /// # Returns
    ///
    /// Count of entries removed from cache.
    ///
    /// # Notes
    ///
    /// - Called after backup or checkpoint to reclaim cache memory
    /// - Only removes entries known to be persisted
    /// - Does NOT remove entries with commit_lsn >= before_lsn (might still be needed)
    pub fn cleanup_before_lsn(&self, before_lsn: Lsn) -> usize {
        self.commit_log.cleanup_entries(before_lsn)
    }

    /// Get access to the underlying CommitLog for direct cache queries.
    ///
    /// # Caution
    ///
    /// Direct access bypasses the facade's safety checks.
    /// Prefer using facade methods when possible.
    pub fn commit_log(&self) -> Arc<CommitLog> {
        Arc::clone(&self.commit_log)
    }
}

impl Default for CommitLogFacade {
    fn default() -> Self {
        Self::new()
    }
}
