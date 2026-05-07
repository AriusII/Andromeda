//! CommitLog Facade: Orchestration Layer for Transaction Commit Recording
//!
//! This module implements the `CommitLogFacade` which coordinates between
//! the in-memory commit log cache and the write-ahead log (WAL) persistence
//! layer. It ensures strict ordering invariants for durability and visibility.
//!
//! **Responsibility:** Orchestrate commit recording, durability confirmation,
//! and visibility publishing with coordinated WAL integration.
//!
//! # Durability Invariants (Strict Ordering)
//!
//! **Invariant 1: Commit log entry created for an assigned WAL commit LSN**
//! - Caller provides the LSN assigned to the corresponding WAL commit record
//! - CommitLogEntry stores that LSN before any visibility publication
//! - Never defer LSN assignment
//!
//! **Invariant 2: Durable flag set AFTER WAL flush confirmation**
//! - Entry initially created with `is_durable() == false`
//! - Only set to `true` after WAL flush operation completes
//! - Transition is atomic (never reverted)
//!
//! **Invariant 3: Visibility flag set AFTER durable flag is true**
//! - `make_visible()` only callable if `confirm_durable()` has completed
//! - TransactionStatusTable updated ONLY when entry is durable
//! - Ensures "visible commit ≡ durable WAL" doctrine
//!
//! **Invariant 4: No entry without WAL record**
//! - Every CommitLogEntry has corresponding WAL record with LSN
//! - LSN is immutable and references persisted state
//! - GC eligibility determined by LSN position relative to snapshots
//!
//! # Thread Safety
//!
//! - CommitLog uses DashMap for lock-free concurrent access per entry
//! - WAL writes must be serialized (single-threaded append)
//! - Durable/visibility transitions are atomic per entry
//!
//! # Performance Notes
//!
//! - Target: <10μs for record_commit + confirm_durable round-trip
//! - Entry lookup is O(1) average via DashMap
//! - Cleanup batches old entries to amortize work

use andromeda_core::{AndromedaResult, TransactionId};

use crate::Lsn;

use super::commit_log_entry::{CommitLog, CommitLogEntry, Timestamp, transaction_error};

/// Facade coordinating the commit-log cache with WAL durability evidence.
///
/// The facade owns the ordering gate between commit-record assignment, WAL
/// flush confirmation, and visibility publication. It intentionally does not
/// expose direct mutable access to the underlying cache.
pub struct CommitLogFacade {
    commit_log: CommitLog,
}

impl CommitLogFacade {
    /// Create a new `CommitLogFacade` with an empty commit-log cache.
    pub fn new() -> Self {
        Self {
            commit_log: CommitLog::new(),
        }
    }

    /// Record a transaction commit after the WAL commit LSN has been assigned.
    ///
    /// The entry is cached with `is_durable() == false`; callers must confirm
    /// the WAL flush before publishing visibility.
    pub fn record_commit(
        &self,
        tx_id: TransactionId,
        commit_lsn: Lsn,
        visible_ts: Timestamp,
    ) -> AndromedaResult<()> {
        let entry = CommitLogEntry::new(tx_id, commit_lsn, visible_ts)?;
        self.commit_log.record_commit(entry)
    }

    /// Confirm that the WAL commit record has been flushed durably.
    pub fn confirm_durable(&self, tx_id: TransactionId) -> AndromedaResult<()> {
        self.commit_log.confirm_durable(tx_id)
    }

    /// Validate that a transaction may become visible to readers.
    ///
    /// This method is deliberately a gate, not the publisher itself. Callers
    /// update the transaction status table only after this method returns.
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

    /// Query cached commit durability evidence.
    pub fn query_status(&self, tx_id: TransactionId) -> AndromedaResult<Option<CommitLogEntry>> {
        self.commit_log.query_commit_status(tx_id)
    }

    /// Remove durable entries older than `before_lsn`.
    pub fn cleanup_before_lsn(&self, before_lsn: Lsn) -> usize {
        self.commit_log.cleanup_entries(before_lsn)
    }
}

impl Default for CommitLogFacade {
    fn default() -> Self {
        Self::new()
    }
}
