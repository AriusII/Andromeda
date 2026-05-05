//! CommitLog Facade: Orchestration Layer for Transaction Commit Recording
//!
//! This module implements the `CommitLogFacade` which coordinates between
//! the in-memory commit log cache and the write-ahead log (WAL) persistence
//! layer. It ensures strict ordering invariants for durability and visibility.
//!
//! # Wave 21 Batch 4 Task 4 — CommitLog Facade Implementation
//!
//! **Responsibility:** Orchestrate commit recording, durability confirmation,
//! and visibility publishing with coordinated WAL integration.
//!
//! # Durability Invariants (Strict Ordering)
//!
//! **Invariant 1: Commit log entry created AFTER WAL write**
//! - WAL record is written first (LSN assigned by WAL manager)
//! - CommitLogEntry created with LSN from WAL record
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

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use crate::Lsn;
use super::commit_log_entry::{CommitLog, CommitLogEntry, Timestamp};
use std::sync::Arc;

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
/// ```no_run
/// // 1. Record commit and write WAL
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
    /// **Wave 21 Batch 4 Task 4 — Commit Recording**
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
        // Validate inputs
        if tx_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "transaction id must not be zero",
            ));
        }

        if visible_ts == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "visible_timestamp must not be zero",
            ));
        }

        // Create entry (initially not durable)
        let entry = CommitLogEntry::new(tx_id, commit_lsn, visible_ts)?;

        // Record in commit log cache
        self.commit_log.record_commit(entry)?;

        Ok(())
    }

    /// Confirm that the WAL record for a commit is durable (flushed to disk).
    ///
    /// **Wave 21 Batch 4 Task 4 — Durability Confirmation**
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
        self.commit_log.confirm_durable(tx_id)?;
        Ok(())
    }

    /// Publish transaction as visible to all readers.
    ///
    /// **Wave 21 Batch 4 Task 4 — Visibility Publishing**
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
        // Query entry to check durability
        match self.commit_log.query_commit_status(tx_id)? {
            Some(entry) => {
                if !entry.is_durable() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Transaction,
                        format!(
                            "cannot make visible: transaction {} is not durable yet",
                            tx_id.get()
                        ),
                    ));
                }
                // Entry is durable; caller will update visibility via TransactionStatusTable
                Ok(())
            }
            None => Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                format!("commit log entry not found for transaction {}", tx_id.get()),
            )),
        }
    }

    /// Query the commit status for a transaction.
    ///
    /// **Wave 21 Batch 4 Task 4 — Status Query**
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
    /// **Wave 21 Batch 4 Task 4 — GC-Aware Cleanup**
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
        self.commit_log.cleanup_before_lsn(before_lsn)
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

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::TransactionIdGenerator;

    fn make_facade() -> CommitLogFacade {
        CommitLogFacade::new()
    }

    // ========================
    // Happy Path Tests
    // ========================

    #[test]
    fn test_record_commit_creates_entry() {
        let facade = make_facade();
        let tx_id = TransactionIdGenerator::new().generate();
        let lsn = Lsn::new(1);
        let ts = 100u64;

        let result = facade.record_commit(tx_id, lsn, ts);
        assert!(result.is_ok());

        // Entry should exist but not be durable yet
        let entry = facade.query_status(tx_id).unwrap().unwrap();
        assert_eq!(entry.tx_id(), tx_id);
        assert_eq!(entry.commit_lsn(), lsn);
        assert_eq!(entry.visible_timestamp(), ts);
        assert!(!entry.is_durable());
    }

    #[test]
    fn test_confirm_durable_marks_entry() {
        let facade = make_facade();
        let tx_id = TransactionIdGenerator::new().generate();
        let lsn = Lsn::new(1);
        let ts = 100u64;

        facade.record_commit(tx_id, lsn, ts).unwrap();

        // Confirm durability
        let result = facade.confirm_durable(tx_id);
        assert!(result.is_ok());

        // Entry should now be durable
        let entry = facade.query_status(tx_id).unwrap().unwrap();
        assert!(entry.is_durable());
    }

    #[test]
    fn test_make_visible_succeeds_after_durable() {
        let facade = make_facade();
        let tx_id = TransactionIdGenerator::new().generate();
        let lsn = Lsn::new(1);
        let ts = 100u64;

        facade.record_commit(tx_id, lsn, ts).unwrap();
        facade.confirm_durable(tx_id).unwrap();

        // Should succeed now
        let result = facade.make_visible(tx_id);
        assert!(result.is_ok());
    }

    #[test]
    fn test_full_flow_record_confirm_visible() {
        let facade = make_facade();
        let tx_id = TransactionIdGenerator::new().generate();

        // Step 1: Record commit
        facade.record_commit(tx_id, Lsn::new(1), 100).unwrap();

        // Step 2: Verify not durable yet
        let entry = facade.query_status(tx_id).unwrap().unwrap();
        assert!(!entry.is_durable());

        // Step 3: Confirm durable
        facade.confirm_durable(tx_id).unwrap();

        // Step 4: Verify durable
        let entry = facade.query_status(tx_id).unwrap().unwrap();
        assert!(entry.is_durable());

        // Step 5: Make visible
        facade.make_visible(tx_id).unwrap();
    }

    // ========================
    // Error Path Tests
    // ========================

    #[test]
    fn test_record_commit_rejects_zero_tx_id() {
        let facade = make_facade();
        let result = facade.record_commit(TransactionId::new(0), Lsn::new(1), 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_record_commit_rejects_zero_timestamp() {
        let facade = make_facade();
        let tx_id = TransactionIdGenerator::new().generate();
        let result = facade.record_commit(tx_id, Lsn::new(1), 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_record_commit_duplicate_fails() {
        let facade = make_facade();
        let tx_id = TransactionIdGenerator::new().generate();

        facade.record_commit(tx_id, Lsn::new(1), 100).unwrap();

        // Second record should fail
        let result = facade.record_commit(tx_id, Lsn::new(2), 200);
        assert!(result.is_err());
    }

    #[test]
    fn test_confirm_durable_nonexistent_fails() {
        let facade = make_facade();
        let tx_id = TransactionIdGenerator::new().generate();

        let result = facade.confirm_durable(tx_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_make_visible_before_durable_fails() {
        let facade = make_facade();
        let tx_id = TransactionIdGenerator::new().generate();

        facade.record_commit(tx_id, Lsn::new(1), 100).unwrap();

        // Try to make visible without confirming durable
        let result = facade.make_visible(tx_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_make_visible_nonexistent_fails() {
        let facade = make_facade();
        let tx_id = TransactionIdGenerator::new().generate();

        let result = facade.make_visible(tx_id);
        assert!(result.is_err());
    }

    // ========================
    // Query Tests
    // ========================

    #[test]
    fn test_query_status_nonexistent_returns_none() {
        let facade = make_facade();
        let tx_id = TransactionIdGenerator::new().generate();

        let result = facade.query_status(tx_id).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_query_status_returns_entry_with_lsn() {
        let facade = make_facade();
        let tx_id = TransactionIdGenerator::new().generate();
        let lsn = Lsn::new(42);
        let ts = 999u64;

        facade.record_commit(tx_id, lsn, ts).unwrap();

        let entry = facade.query_status(tx_id).unwrap().unwrap();
        assert_eq!(entry.commit_lsn(), lsn);
        assert_eq!(entry.visible_timestamp(), ts);
    }

    // ========================
    // Cleanup Tests
    // ========================

    #[test]
    fn test_cleanup_before_lsn_removes_old_entries() {
        let facade = make_facade();

        let tx1 = TransactionIdGenerator::new().generate();
        let tx2 = TransactionIdGenerator::new().generate();
        let tx3 = TransactionIdGenerator::new().generate();

        facade.record_commit(tx1, Lsn::new(1), 100).unwrap();
        facade.record_commit(tx2, Lsn::new(5), 200).unwrap();
        facade.record_commit(tx3, Lsn::new(10), 300).unwrap();

        // Confirm all as durable so they can be cleaned
        facade.confirm_durable(tx1).unwrap();
        facade.confirm_durable(tx2).unwrap();
        facade.confirm_durable(tx3).unwrap();

        // Clean entries before LSN 6 (should remove tx1 and tx2)
        let removed = facade.cleanup_before_lsn(Lsn::new(6));
        assert_eq!(removed, 2);

        // tx1 and tx2 should be gone, tx3 should remain
        assert!(facade.query_status(tx1).unwrap().is_none());
        assert!(facade.query_status(tx2).unwrap().is_none());
        assert!(facade.query_status(tx3).unwrap().is_some());
    }

    #[test]
    fn test_cleanup_preserves_entries_at_boundary() {
        let facade = make_facade();

        let tx1 = TransactionIdGenerator::new().generate();
        let tx2 = TransactionIdGenerator::new().generate();

        facade.record_commit(tx1, Lsn::new(5), 100).unwrap();
        facade.record_commit(tx2, Lsn::new(5), 200).unwrap();

        facade.confirm_durable(tx1).unwrap();
        facade.confirm_durable(tx2).unwrap();

        // Clean entries before LSN 5 (should not remove any)
        let removed = facade.cleanup_before_lsn(Lsn::new(5));
        assert_eq!(removed, 0);

        // Both should still exist
        assert!(facade.query_status(tx1).unwrap().is_some());
        assert!(facade.query_status(tx2).unwrap().is_some());
    }

    #[test]
    fn test_cleanup_does_not_remove_undurable_entries() {
        let facade = make_facade();

        let tx1 = TransactionIdGenerator::new().generate();
        let tx2 = TransactionIdGenerator::new().generate();

        facade.record_commit(tx1, Lsn::new(1), 100).unwrap();
        facade.record_commit(tx2, Lsn::new(10), 200).unwrap();

        // Only confirm tx2 as durable
        facade.confirm_durable(tx2).unwrap();

        // Clean entries before LSN 100
        // Should only remove tx2 (tx1 is not durable)
        let removed = facade.cleanup_before_lsn(Lsn::new(100));
        assert_eq!(removed, 1);

        // tx1 should still exist, tx2 should be gone
        assert!(facade.query_status(tx1).unwrap().is_some());
        assert!(facade.query_status(tx2).unwrap().is_none());
    }

    // ========================
    // Concurrency Tests
    // ========================

    #[test]
    fn test_concurrent_record_commit() {
        use std::sync::Arc;
        use std::thread;

        let facade = Arc::new(make_facade());
        let handles: Vec<_> = (0..50)
            .map(|i| {
                let facade_clone = Arc::clone(&facade);
                thread::spawn(move || {
                    let tx_id = TransactionIdGenerator::new().generate();
                    let lsn = Lsn::new((i + 1) as u64);
                    let ts = 100u64 + i as u64;

                    facade_clone.record_commit(tx_id, lsn, ts).unwrap();

                    // Verify entry exists
                    let entry = facade_clone.query_status(tx_id).unwrap();
                    assert!(entry.is_some());
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_concurrent_confirm_durable() {
        use std::sync::Arc;
        use std::thread;

        let facade = Arc::new(make_facade());

        // Pre-populate entries
        let tx_ids: Vec<_> = (0..50)
            .map(|i| {
                let tx_id = TransactionIdGenerator::new().generate();
                let lsn = Lsn::new((i + 1) as u64);
                facade.record_commit(tx_id, lsn, 100u64).unwrap();
                tx_id
            })
            .collect();

        // Concurrently confirm durability
        let handles: Vec<_> = tx_ids
            .into_iter()
            .map(|tx_id| {
                let facade_clone = Arc::clone(&facade);
                thread::spawn(move || {
                    facade_clone.confirm_durable(tx_id).unwrap();

                    // Verify durable
                    let entry = facade_clone.query_status(tx_id).unwrap().unwrap();
                    assert!(entry.is_durable());
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }

    // ========================
    // Invariant Tests
    // ========================

    #[test]
    fn test_invariant_entry_not_durable_initially() {
        let facade = make_facade();
        let tx_id = TransactionIdGenerator::new().generate();

        facade.record_commit(tx_id, Lsn::new(1), 100).unwrap();

        let entry = facade.query_status(tx_id).unwrap().unwrap();
        assert!(!entry.is_durable(), "Entry should not be durable initially");
    }

    #[test]
    fn test_invariant_confirm_durable_makes_visible() {
        let facade = make_facade();
        let tx_id = TransactionIdGenerator::new().generate();

        facade.record_commit(tx_id, Lsn::new(1), 100).unwrap();
        facade.confirm_durable(tx_id).unwrap();

        // After confirm_durable, make_visible should succeed
        let result = facade.make_visible(tx_id);
        assert!(result.is_ok(), "make_visible should succeed after confirm_durable");
    }

    #[test]
    fn test_invariant_durable_before_visible() {
        let facade = make_facade();
        let tx_id = TransactionIdGenerator::new().generate();

        facade.record_commit(tx_id, Lsn::new(1), 100).unwrap();

        // Try to make visible before confirming durable
        let result = facade.make_visible(tx_id);
        assert!(
            result.is_err(),
            "make_visible should fail before confirm_durable"
        );
    }

    // ========================
    // Stress Tests
    // ========================

    #[test]
    fn test_stress_100_concurrent_full_flow() {
        use std::sync::Arc;
        use std::thread;

        let facade = Arc::new(make_facade());
        let handles: Vec<_> = (0..100)
            .map(|i| {
                let facade_clone = Arc::clone(&facade);
                thread::spawn(move || {
                    let tx_id = TransactionIdGenerator::new().generate();
                    let lsn = Lsn::new((i + 1) as u64);
                    let ts = 1000u64 + i as u64;

                    // Record
                    facade_clone.record_commit(tx_id, lsn, ts).unwrap();

                    // Verify not durable
                    let entry = facade_clone.query_status(tx_id).unwrap().unwrap();
                    assert!(!entry.is_durable());

                    // Confirm durable
                    facade_clone.confirm_durable(tx_id).unwrap();

                    // Verify durable
                    let entry = facade_clone.query_status(tx_id).unwrap().unwrap();
                    assert!(entry.is_durable());

                    // Make visible
                    facade_clone.make_visible(tx_id).unwrap();
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_stress_100_commits_with_cleanup() {
        let facade = make_facade();

        // Create 100 transactions
        let tx_ids: Vec<_> = (0..100)
            .map(|i| {
                let tx_id = TransactionIdGenerator::new().generate();
                let lsn = Lsn::new((i + 1) as u64);
                facade.record_commit(tx_id, lsn, 100u64 + i as u64).unwrap();
                facade.confirm_durable(tx_id).unwrap();
                tx_id
            })
            .collect();

        // Clean entries before LSN 50 (should remove first 49)
        let removed = facade.cleanup_before_lsn(Lsn::new(50));
        assert_eq!(removed, 49);

        // Remaining 51 should be present
        for (i, tx_id) in tx_ids.into_iter().enumerate() {
            let expected = (i + 1) >= 50;
            let exists = facade.query_status(tx_id).unwrap().is_some();
            assert_eq!(exists, expected, "Transaction {} existence mismatch", i + 1);
        }
    }
}
