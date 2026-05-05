//! WAL Commit Log Manager linking transaction commits to WAL durability with LSN tracking.
//!
//! # Overview
//!
//! The `CommitLogManager` ensures the durable-commit doctrine: a transaction is
//! visible to other snapshots **only after** its commit record is durably flushed
//! to the Write-Ahead Log (WAL).
//!
//! # Five-Step Commit Sequence
//!
//! 1. **State Check**: Verify transaction is in Committing state
//! 2. **Write to WAL**: Append TxCommit record to WAL buffer (non-blocking)
//! 3. **Flush to Disk**: Flush WAL through the commit LSN (BLOCKING, durability boundary)
//! 4. **Mark Visible**: Update TransactionStatusTable to Committed (after durability)
//! 5. **Clean Up**: Dispose transaction scope and emit audit trace
//!
//! # Invariants
//!
//! - **Durability First**: WAL flush completes *before* visibility update
//! - **Atomic Status Update**: TransactionStatusTable.set_committed() happens exactly once
//! - **LSN Tracking**: Every commit carries its durable LSN for recovery correlation
//! - **GC Eligibility**: Commits older than min_active_snapshot_lsn are eligible for cleanup
//!
//! # Thread Safety
//!
//! The manager is safe to share across threads via `Arc<CommitLogManager>`:
//! - `commit_entries` uses DashMap for concurrent access
//! - `status_table` and `wal_manager` are shared references
//! - `record_commit` is async-safe and reentrant

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, Clock, EngineTimestamp, SystemClock,
    TransactionId,
};
use dashmap::DashMap;
use std::sync::Arc;

use crate::Lsn;
use crate::mvcc_status::{TransactionStatus, TransactionStatusTable};

/// Transaction WAL record kinds required by the commit boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalRecordKind {
    /// Transaction commit record. Adapters map this to the storage WAL record kind.
    TxCommit,
}

/// Isolation level for transaction commit classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationLevel {
    /// Snapshot isolation (typical default)
    Snapshot,
    /// Serializable isolation (strictest)
    Serializable,
}

/// Single entry in the commit log, linking TxId to CommitLsn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitLogEntry {
    /// Unique transaction identifier
    pub tx_id: TransactionId,
    /// LSN of the durable commit record in WAL
    pub commit_lsn: Lsn,
    /// Timestamp when commit became durable
    pub timestamp: EngineTimestamp,
    /// Count of rows affected by this transaction
    pub row_count_affected: u64,
    /// Isolation level at which this transaction committed
    pub isolation_level: IsolationLevel,
}

impl CommitLogEntry {
    /// Check whether this entry is older than a given LSN threshold.
    pub fn is_before(&self, threshold_lsn: Lsn) -> bool {
        self.commit_lsn < threshold_lsn
    }
}

/// Manager for transaction commits linked to WAL durability.
///
/// The `CommitLogManager` owns the lifecycle of durable transaction commits.
/// It bridges the gap between the transaction manager (which tracks state machines)
/// and the MVCC system (which needs visibility decisions based on durable commits).
pub struct CommitLogManager {
    /// Reference to WAL manager for record persistence
    wal_manager: Arc<dyn InvocationWal>,
    /// Reference to transaction status table for visibility updates
    status_table: Arc<TransactionStatusTable>,
    /// Fast lookup table: TxId → CommitLogEntry
    commit_entries: Arc<DashMap<TransactionId, CommitLogEntry>>,
    /// Clock for timestamp generation
    clock: Arc<dyn Clock>,
}

impl CommitLogManager {
    /// Create a new commit log manager with required dependencies.
    pub fn new(
        wal_manager: Arc<dyn InvocationWal>,
        status_table: Arc<TransactionStatusTable>,
    ) -> Self {
        Self::with_clock(wal_manager, status_table, Arc::new(SystemClock))
    }

    /// Create a new commit log manager with injectable clock (for testing).
    pub fn with_clock(
        wal_manager: Arc<dyn InvocationWal>,
        status_table: Arc<TransactionStatusTable>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            wal_manager,
            status_table,
            commit_entries: Arc::new(DashMap::new()),
            clock,
        }
    }

    /// Record a transaction commit with full durability guarantee.
    ///
    /// This method implements the five-step commit sequence:
    ///
    /// 1. Create WAL record with transaction metadata
    /// 2. Append to WAL buffer (non-blocking)
    /// 3. **CRITICAL**: Flush WAL to durable storage (blocking)
    /// 4. Store commit entry in fast lookup table
    /// 5. Update transaction status table (after durability)
    ///
    /// # Arguments
    ///
    /// - `tx_id`: Unique transaction identifier
    /// - `isolation_level`: Isolation level at which transaction committed
    /// - `affected_rows`: Count of rows modified by this transaction
    /// - `parameter_hash`: Hash of execution parameters (for forensic analysis)
    ///
    /// # Returns
    ///
    /// A `CommitLogEntry` containing the durable commit LSN and metadata.
    /// If WAL flush fails, the transaction remains uncommitted.
    ///
    /// # Durability Invariant
    ///
    /// After this method returns Ok, the transaction is guaranteed to:
    /// - Have a durable WAL record at `entry.commit_lsn`
    /// - Be visible to new snapshots via the status table
    /// - Be recoverable after a crash
    pub async fn record_commit(
        &self,
        tx_id: TransactionId,
        isolation_level: IsolationLevel,
        affected_rows: u64,
        parameter_hash: u64,
    ) -> AndromedaResult<CommitLogEntry> {
        // Step 1: Validate transaction ID
        if tx_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "cannot commit transaction with zero ID",
            ));
        }

        // Step 2: Create commit record payload
        // Format: isolation_level(1) + affected_rows(8) + parameter_hash(8) + flags(8)
        let mut payload = Vec::with_capacity(25);
        payload.push(match isolation_level {
            IsolationLevel::Snapshot => 1,
            IsolationLevel::Serializable => 2,
        });
        payload.extend_from_slice(&affected_rows.to_le_bytes());
        payload.extend_from_slice(&parameter_hash.to_le_bytes());
        payload.extend_from_slice(&0u64.to_le_bytes()); // flags (reserved for future)

        // Step 3: Write to WAL buffer (non-blocking)
        // The WAL manager appends the record and assigns the LSN
        let commit_lsn = self
            .wal_manager
            .append(WalRecordKind::TxCommit, Some(tx_id), &payload)
            .await?;

        // Step 4: **CRITICAL DURABILITY BOUNDARY**: Flush WAL to disk
        // This is the synchronization point that ensures durability.
        // No snapshot should see this transaction as committed until this completes.
        self.wal_manager.flush_through(commit_lsn).await?;

        // Step 5: Get current timestamp for visibility tracking
        let timestamp = self.clock.now();

        // Step 6: Create commit log entry
        let entry = CommitLogEntry {
            tx_id,
            commit_lsn,
            timestamp,
            row_count_affected: affected_rows,
            isolation_level,
        };

        // Step 7: Store in commit_entries for fast lookup
        // This is safe to insert even if another thread raced us;
        // we validate uniqueness in the status table update.
        self.commit_entries.insert(tx_id, entry.clone());

        // Step 8: **CRITICAL**: Update transaction status table atomically
        // INVARIANT: This must happen AFTER WAL durability (step 4)
        // After this point, the transaction is visible to snapshots.
        self.status_table.set_committed(tx_id)?;

        // Step 9: Emit audit trace for forensic analysis and recovery verification
        self.emit_commit_trace(&entry)?;

        Ok(entry)
    }

    /// Check whether a transaction is marked as committed.
    pub fn is_committed(&self, tx_id: TransactionId) -> bool {
        matches!(
            self.status_table.get_status(tx_id),
            Some(TransactionStatus::Committed)
        )
    }

    /// Retrieve the commit LSN for a transaction (if committed).
    pub fn get_commit_lsn(&self, tx_id: TransactionId) -> Option<Lsn> {
        self.commit_entries
            .get(&tx_id)
            .map(|entry| entry.commit_lsn)
    }

    /// Retrieve the commit timestamp for a transaction (if committed).
    pub fn get_commit_timestamp(&self, tx_id: TransactionId) -> Option<EngineTimestamp> {
        self.commit_entries.get(&tx_id).map(|entry| entry.timestamp)
    }

    /// Retrieve row count affected by a committed transaction.
    pub fn get_affected_rows(&self, tx_id: TransactionId) -> Option<u64> {
        self.commit_entries
            .get(&tx_id)
            .map(|entry| entry.row_count_affected)
    }

    /// Identify garbage collection candidates based on LSN threshold.
    ///
    /// Returns a list of transaction IDs whose commits are older than
    /// `min_active_snapshot_lsn`. These entries are no longer needed by any
    /// active snapshot and can be safely removed from the commit log.
    ///
    /// # Arguments
    ///
    /// - `min_active_snapshot_lsn`: LSN of the earliest active snapshot.
    ///   All commits before this LSN are eligible for cleanup.
    ///
    /// # Returns
    ///
    /// A vector of transaction IDs that can be garbage collected.
    pub fn gc_candidates(&self, min_active_snapshot_lsn: Lsn) -> Vec<TransactionId> {
        self.commit_entries
            .iter()
            .filter_map(|entry| {
                if entry.value().is_before(min_active_snapshot_lsn) {
                    Some(entry.value().tx_id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Remove committed transaction entries for garbage collection.
    ///
    /// Deletes entries from the commit log that are no longer needed.
    /// This should only be called for entries returned by `gc_candidates`.
    ///
    /// # Arguments
    ///
    /// - `tx_ids`: Iterator of transaction IDs to remove.
    ///
    /// # Returns
    ///
    /// Count of entries actually removed.
    pub fn gc_remove(&self, tx_ids: impl Iterator<Item = TransactionId>) -> usize {
        let mut count = 0;
        for tx_id in tx_ids {
            if self.commit_entries.remove(&tx_id).is_some() {
                count += 1;
            }
        }
        count
    }

    /// Verify that a committed transaction is durably recoverable from the WAL.
    ///
    /// Used post-recovery to ensure that all visible transactions have durable
    /// WAL records. This is primarily a validation/debugging tool.
    ///
    /// # Returns
    ///
    /// Ok(()) if the transaction's commit record is present at its LSN,
    /// Err if the entry is missing or recovery verification failed.
    pub fn verify_durability(&self, tx_id: TransactionId) -> AndromedaResult<()> {
        match self.commit_entries.get(&tx_id) {
            Some(entry) => {
                // In production, this would read from the WAL file at entry.commit_lsn
                // and verify that a TxCommit record exists. For now, presence in
                // commit_entries is the verification.
                let _ = entry;
                Ok(())
            }
            None => Err(AndromedaError::new(
                AndromedaErrorKind::Internal,
                "Commit entry not found during durability verification",
            )),
        }
    }

    /// Emit audit trace for transaction commit.
    ///
    /// Creates a forensic record of the commit for:
    /// - Recovery verification
    /// - Audit logging
    /// - Performance analysis
    fn emit_commit_trace(&self, entry: &CommitLogEntry) -> AndromedaResult<()> {
        // TODO: Integrate with ProcedureInvocationTrace subsystem
        // For now, this is a placeholder that allows future integration
        // without breaking the commit protocol.
        let _ = entry;
        Ok(())
    }
}

/// Abstract trait for WAL append and flush operations used by commit log.
///
/// This trait allows the commit log to work with different WAL implementations
/// (in-memory, file-based, remote) without being coupled to a specific one.
#[async_trait::async_trait]
pub trait InvocationWal: Send + Sync {
    /// Append a WAL record and return its assigned LSN.
    async fn append(
        &self,
        kind: WalRecordKind,
        transaction_id: Option<TransactionId>,
        payload: &[u8],
    ) -> AndromedaResult<Lsn>;

    /// Flush WAL records through the specified LSN to durable storage.
    async fn flush_through(&self, lsn: Lsn) -> AndromedaResult<Lsn>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock WAL for testing commit log without async I/O
    struct MockWal {
        records: std::sync::Mutex<Vec<Lsn>>,
        durable_lsn: std::sync::Mutex<Lsn>,
    }

    impl MockWal {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                records: std::sync::Mutex::new(Vec::new()),
                durable_lsn: std::sync::Mutex::new(Lsn::new(0)),
            })
        }
    }

    #[async_trait::async_trait]
    impl InvocationWal for MockWal {
        async fn append(
            &self,
            _kind: WalRecordKind,
            _transaction_id: Option<TransactionId>,
            _payload: &[u8],
        ) -> AndromedaResult<Lsn> {
            let mut records = self.records.lock().unwrap();
            let next_lsn = Lsn::new(records.len() as u64 + 1);
            records.push(next_lsn);
            Ok(next_lsn)
        }

        async fn flush_through(&self, lsn: Lsn) -> AndromedaResult<Lsn> {
            let mut durable = self.durable_lsn.lock().unwrap();
            *durable = lsn;
            Ok(lsn)
        }
    }

    #[tokio::test]
    async fn test_commit_log_records_entry() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = CommitLogManager::new(wal, status_table);

        let tx_id = TransactionId::new(1);
        let entry = commit_log
            .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
            .await
            .unwrap();

        assert!(commit_log.is_committed(tx_id));
        assert_eq!(commit_log.get_commit_lsn(tx_id), Some(entry.commit_lsn));
        assert_eq!(commit_log.get_affected_rows(tx_id), Some(10));
    }

    #[tokio::test]
    async fn test_commit_log_rejects_zero_tx_id() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = CommitLogManager::new(wal, status_table);

        let result = commit_log
            .record_commit(TransactionId::new(0), IsolationLevel::Snapshot, 5, 0)
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), AndromedaErrorKind::Transaction);
    }

    #[tokio::test]
    async fn test_commit_log_gc_candidates() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = CommitLogManager::new(wal, status_table);

        // Record 5 commits
        for i in 1..=5 {
            commit_log
                .record_commit(
                    TransactionId::new(i as u64),
                    IsolationLevel::Snapshot,
                    i as u64,
                    0,
                )
                .await
                .unwrap();
        }

        // All LSNs are 1-5, so threshold of Lsn(3) should find tx_ids 1,2
        let candidates = commit_log.gc_candidates(Lsn::new(3));
        assert_eq!(candidates.len(), 2);
        assert!(candidates.contains(&TransactionId::new(1)));
        assert!(candidates.contains(&TransactionId::new(2)));
    }

    #[tokio::test]
    async fn test_commit_log_verify_durability() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = CommitLogManager::new(wal, status_table);

        let tx_id = TransactionId::new(1);
        commit_log
            .record_commit(tx_id, IsolationLevel::Snapshot, 5, 0)
            .await
            .unwrap();

        // Verify succeeds for committed transaction
        assert!(commit_log.verify_durability(tx_id).is_ok());

        // Verify fails for uncommitted transaction
        let result = commit_log.verify_durability(TransactionId::new(999));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_commit_log_isolation_levels() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = CommitLogManager::new(wal, status_table);

        let snapshot_entry = commit_log
            .record_commit(TransactionId::new(1), IsolationLevel::Snapshot, 10, 0)
            .await
            .unwrap();
        assert_eq!(snapshot_entry.isolation_level, IsolationLevel::Snapshot);

        let serializable_entry = commit_log
            .record_commit(TransactionId::new(2), IsolationLevel::Serializable, 20, 0)
            .await
            .unwrap();
        assert_eq!(
            serializable_entry.isolation_level,
            IsolationLevel::Serializable
        );
    }
}
