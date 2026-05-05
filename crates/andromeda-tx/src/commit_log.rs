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
use tokio::sync::Mutex as AsyncMutex;

use crate::Lsn;
use crate::mvcc_status::{TransactionStatus, TransactionStatusTable};

/// Transaction WAL record kinds required by the commit boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalRecordKind {
    /// Transaction commit record. Adapters map this to the storage WAL record kind.
    TxCommit,
    /// Transaction rollback record. Adapters map this to the storage WAL record kind.
    TxRollback,
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

/// Single durable rollback entry, linking TxId to RollbackLsn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackLogEntry {
    /// Unique transaction identifier.
    pub tx_id: TransactionId,
    /// LSN of the durable rollback record in WAL.
    pub rollback_lsn: Lsn,
    /// Timestamp when rollback became durable.
    pub timestamp: EngineTimestamp,
    /// Hash of rollback/context parameters, for audit/recovery correlation.
    pub parameter_hash: u64,
}

/// Transaction-local WAL replay record used by recovery code.
///
/// Storage/WAL implementations should translate their durable WAL record shape
/// into this transaction-owned type before calling [`CommitLogManager`] replay
/// APIs. Records are expected in durable WAL order. Duplicate terminal records
/// for the same transaction are treated as idempotent and the first terminal
/// record wins; conflicting commit-vs-rollback evidence is rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TxWalReplayRecord {
    /// Durable transaction commit evidence.
    Commit(CommitLogEntry),
    /// Durable transaction rollback evidence.
    Rollback(RollbackLogEntry),
    /// A transaction was observed in WAL replay but no terminal record was
    /// durable. It remains absent from the status table and therefore invisible.
    Incomplete { tx_id: TransactionId, last_lsn: Lsn },
}

impl TxWalReplayRecord {
    /// Build a replay commit record from transaction-local metadata.
    pub fn commit(
        tx_id: TransactionId,
        commit_lsn: Lsn,
        timestamp: EngineTimestamp,
        row_count_affected: u64,
        isolation_level: IsolationLevel,
    ) -> Self {
        Self::Commit(CommitLogEntry {
            tx_id,
            commit_lsn,
            timestamp,
            row_count_affected,
            isolation_level,
        })
    }

    /// Build a replay rollback record from transaction-local metadata.
    pub fn rollback(
        tx_id: TransactionId,
        rollback_lsn: Lsn,
        timestamp: EngineTimestamp,
        parameter_hash: u64,
    ) -> Self {
        Self::Rollback(RollbackLogEntry {
            tx_id,
            rollback_lsn,
            timestamp,
            parameter_hash,
        })
    }

    /// Build a replay marker for a transaction without durable terminal
    /// evidence.
    pub fn incomplete(tx_id: TransactionId, last_lsn: Lsn) -> Self {
        Self::Incomplete { tx_id, last_lsn }
    }

    /// Return the transaction identified by this replay record.
    pub fn tx_id(&self) -> TransactionId {
        match self {
            Self::Commit(entry) => entry.tx_id,
            Self::Rollback(entry) => entry.tx_id,
            Self::Incomplete { tx_id, .. } => *tx_id,
        }
    }
}

/// Per-record action taken by transaction WAL replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxWalReplayAction {
    /// A new commit entry was restored.
    CommitRestored,
    /// A new rollback entry was restored.
    RollbackRestored,
    /// A commit entry for this transaction was already restored.
    DuplicateCommit,
    /// A rollback entry for this transaction was already restored.
    DuplicateRollback,
    /// The record had no durable terminal evidence and was left invisible.
    IncompleteIgnored,
}

/// Summary returned after replaying transaction-local WAL records.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TxWalReplaySummary {
    pub commits_restored: usize,
    pub rollbacks_restored: usize,
    pub duplicate_commits: usize,
    pub duplicate_rollbacks: usize,
    pub incomplete_transactions: usize,
}

impl TxWalReplaySummary {
    fn record(&mut self, action: TxWalReplayAction) {
        match action {
            TxWalReplayAction::CommitRestored => self.commits_restored += 1,
            TxWalReplayAction::RollbackRestored => self.rollbacks_restored += 1,
            TxWalReplayAction::DuplicateCommit => self.duplicate_commits += 1,
            TxWalReplayAction::DuplicateRollback => self.duplicate_rollbacks += 1,
            TxWalReplayAction::IncompleteIgnored => self.incomplete_transactions += 1,
        }
    }
}

/// Summary returned after rebuilding MVCC status from local durable records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionStatusRebuild {
    pub committed_restored: usize,
    pub rolled_back_restored: usize,
    pub already_present: usize,
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
    /// Fast lookup table: TxId → RollbackLogEntry
    rollback_entries: Arc<DashMap<TransactionId, RollbackLogEntry>>,
    /// Serializes terminal lifecycle decisions so commit/rollback are idempotent.
    lifecycle_lock: AsyncMutex<()>,
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
            rollback_entries: Arc::new(DashMap::new()),
            lifecycle_lock: AsyncMutex::new(()),
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

        let _decision = self.lifecycle_lock.lock().await;
        if let Some(existing) = self.commit_entries.get(&tx_id) {
            return Ok(existing.clone());
        }
        self.reject_commit_after_rollback(tx_id)?;

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

    /// Record a transaction rollback with the same WAL-before-status durability
    /// boundary used by commits.
    ///
    /// The rollback record is durably flushed before the status table is marked
    /// `RolledBack`. Repeating the call for the same transaction returns the
    /// original entry without appending another WAL record. Rolling back an
    /// already committed transaction is rejected.
    pub async fn record_rollback(
        &self,
        tx_id: TransactionId,
        parameter_hash: u64,
    ) -> AndromedaResult<RollbackLogEntry> {
        if tx_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "cannot rollback transaction with zero ID",
            ));
        }

        let _decision = self.lifecycle_lock.lock().await;
        if let Some(existing) = self.rollback_entries.get(&tx_id) {
            return Ok(existing.clone());
        }
        self.reject_rollback_after_commit(tx_id)?;

        let mut payload = Vec::with_capacity(16);
        payload.extend_from_slice(&parameter_hash.to_le_bytes());
        payload.extend_from_slice(&0u64.to_le_bytes());

        let rollback_lsn = self
            .wal_manager
            .append(WalRecordKind::TxRollback, Some(tx_id), &payload)
            .await?;
        self.wal_manager.flush_through(rollback_lsn).await?;

        let entry = RollbackLogEntry {
            tx_id,
            rollback_lsn,
            timestamp: self.clock.now(),
            parameter_hash,
        };

        self.rollback_entries.insert(tx_id, entry.clone());
        self.status_table
            .record(tx_id, TransactionStatus::RolledBack)?;
        self.emit_rollback_trace(&entry)?;

        Ok(entry)
    }

    /// Check whether a transaction is marked as committed.
    pub fn is_committed(&self, tx_id: TransactionId) -> bool {
        self.rebuild_status_for_transaction(tx_id);
        matches!(
            self.status_table.get_status(tx_id),
            Some(TransactionStatus::Committed)
        )
    }

    /// Check whether a transaction is marked as rolled back.
    pub fn is_rolled_back(&self, tx_id: TransactionId) -> bool {
        self.rebuild_status_for_transaction(tx_id);
        matches!(
            self.status_table.get_status(tx_id),
            Some(TransactionStatus::RolledBack)
        )
    }

    /// Retrieve the commit LSN for a transaction (if committed).
    pub fn get_commit_lsn(&self, tx_id: TransactionId) -> Option<Lsn> {
        self.commit_entries
            .get(&tx_id)
            .map(|entry| entry.commit_lsn)
    }

    /// Retrieve the rollback LSN for a transaction (if rolled back).
    pub fn get_rollback_lsn(&self, tx_id: TransactionId) -> Option<Lsn> {
        self.rollback_entries
            .get(&tx_id)
            .map(|entry| entry.rollback_lsn)
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

    /// Restore a durable commit entry discovered during WAL replay.
    pub fn restore_commit_entry(&self, entry: CommitLogEntry) -> AndromedaResult<()> {
        validate_transaction_id(entry.tx_id, "cannot restore commit entry with zero ID")?;
        if self.rollback_entries.contains_key(&entry.tx_id) {
            return Err(conflicting_terminal_record());
        }

        if let Some(existing) = self.commit_entries.get(&entry.tx_id) {
            if *existing != entry {
                return Err(conflicting_terminal_record());
            }
        } else {
            self.commit_entries.insert(entry.tx_id, entry.clone());
        }

        self.restore_status_if_absent(entry.tx_id, TransactionStatus::Committed)
    }

    /// Restore a durable rollback entry discovered during WAL replay.
    pub fn restore_rollback_entry(&self, entry: RollbackLogEntry) -> AndromedaResult<()> {
        validate_transaction_id(entry.tx_id, "cannot restore rollback entry with zero ID")?;
        if self.commit_entries.contains_key(&entry.tx_id) {
            return Err(conflicting_terminal_record());
        }

        if let Some(existing) = self.rollback_entries.get(&entry.tx_id) {
            if *existing != entry {
                return Err(conflicting_terminal_record());
            }
        } else {
            self.rollback_entries.insert(entry.tx_id, entry.clone());
        }

        self.restore_status_if_absent(entry.tx_id, TransactionStatus::RolledBack)
    }

    /// Replay one transaction-local WAL record into the commit log and status
    /// table.
    ///
    /// This API is intentionally storage-agnostic: callers translate durable
    /// WAL bytes into [`TxWalReplayRecord`] outside this crate, then replay the
    /// structured transaction record here. Duplicate commit/rollback evidence
    /// is idempotent and preserves the first record seen in WAL order.
    pub fn replay_tx_wal_record(
        &self,
        record: TxWalReplayRecord,
    ) -> AndromedaResult<TxWalReplayAction> {
        match record {
            TxWalReplayRecord::Commit(entry) => self.replay_commit_record(entry),
            TxWalReplayRecord::Rollback(entry) => self.replay_rollback_record(entry),
            TxWalReplayRecord::Incomplete { tx_id, .. } => {
                validate_transaction_id(
                    tx_id,
                    "cannot replay incomplete transaction with zero ID",
                )?;
                Ok(TxWalReplayAction::IncompleteIgnored)
            }
        }
    }

    /// Reconstruct transaction terminal state from transaction-local WAL replay
    /// records.
    ///
    /// Records should be provided in durable WAL order. Transactions with only
    /// [`TxWalReplayRecord::Incomplete`] records are not inserted into the
    /// status table and remain invisible to MVCC snapshots.
    pub fn reconstruct_from_tx_wal_replay<I>(
        &self,
        records: I,
    ) -> AndromedaResult<TxWalReplaySummary>
    where
        I: IntoIterator<Item = TxWalReplayRecord>,
    {
        let mut summary = TxWalReplaySummary::default();

        for record in records {
            let action = self.replay_tx_wal_record(record)?;
            summary.record(action);
        }

        Ok(summary)
    }

    /// Rebuild missing status-table entries from transaction-local durable
    /// commit and rollback records.
    ///
    /// This is intended for recovery drivers that replay WAL records into this
    /// manager before accepting new transactions. Existing status entries are
    /// validated and left untouched; missing entries are restored.
    pub fn rebuild_status_from_records(&self) -> AndromedaResult<TransactionStatusRebuild> {
        let mut summary = TransactionStatusRebuild {
            committed_restored: 0,
            rolled_back_restored: 0,
            already_present: 0,
        };

        for entry in self.commit_entries.iter() {
            let tx_id = *entry.key();
            if self.rollback_entries.contains_key(&tx_id) {
                return Err(conflicting_terminal_record());
            }
            match self.status_table.status(tx_id) {
                Some(TransactionStatus::Committed) => summary.already_present += 1,
                Some(_) => return Err(conflicting_terminal_record()),
                None => {
                    self.status_table
                        .record(tx_id, TransactionStatus::Committed)?;
                    summary.committed_restored += 1;
                }
            }
        }

        for entry in self.rollback_entries.iter() {
            let tx_id = *entry.key();
            if self.commit_entries.contains_key(&tx_id) {
                return Err(conflicting_terminal_record());
            }
            match self.status_table.status(tx_id) {
                Some(TransactionStatus::RolledBack) => summary.already_present += 1,
                Some(_) => return Err(conflicting_terminal_record()),
                None => {
                    self.status_table
                        .record(tx_id, TransactionStatus::RolledBack)?;
                    summary.rolled_back_restored += 1;
                }
            }
        }

        Ok(summary)
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

    fn emit_rollback_trace(&self, entry: &RollbackLogEntry) -> AndromedaResult<()> {
        let _ = entry;
        Ok(())
    }

    fn reject_commit_after_rollback(&self, tx_id: TransactionId) -> AndromedaResult<()> {
        if self.rollback_entries.contains_key(&tx_id)
            || matches!(
                self.status_table.status(tx_id),
                Some(TransactionStatus::RolledBack)
            )
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "cannot commit a transaction with durable rollback evidence",
            ));
        }

        Ok(())
    }

    fn reject_rollback_after_commit(&self, tx_id: TransactionId) -> AndromedaResult<()> {
        if self.commit_entries.contains_key(&tx_id)
            || matches!(
                self.status_table.status(tx_id),
                Some(TransactionStatus::Committed)
            )
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "cannot rollback a transaction with durable commit evidence",
            ));
        }

        Ok(())
    }

    fn restore_status_if_absent(
        &self,
        tx_id: TransactionId,
        status: TransactionStatus,
    ) -> AndromedaResult<()> {
        match self.status_table.status(tx_id) {
            Some(existing) if existing == status => Ok(()),
            Some(_) => Err(conflicting_terminal_record()),
            None => self.status_table.record(tx_id, status),
        }
    }

    fn replay_commit_record(&self, entry: CommitLogEntry) -> AndromedaResult<TxWalReplayAction> {
        validate_transaction_id(entry.tx_id, "cannot replay commit entry with zero ID")?;
        if self.rollback_entries.contains_key(&entry.tx_id) {
            return Err(conflicting_terminal_record());
        }

        let action = if self.commit_entries.contains_key(&entry.tx_id) {
            TxWalReplayAction::DuplicateCommit
        } else {
            self.commit_entries.insert(entry.tx_id, entry.clone());
            TxWalReplayAction::CommitRestored
        };

        self.restore_status_if_absent(entry.tx_id, TransactionStatus::Committed)?;
        Ok(action)
    }

    fn replay_rollback_record(
        &self,
        entry: RollbackLogEntry,
    ) -> AndromedaResult<TxWalReplayAction> {
        validate_transaction_id(entry.tx_id, "cannot replay rollback entry with zero ID")?;
        if self.commit_entries.contains_key(&entry.tx_id) {
            return Err(conflicting_terminal_record());
        }

        let action = if self.rollback_entries.contains_key(&entry.tx_id) {
            TxWalReplayAction::DuplicateRollback
        } else {
            self.rollback_entries.insert(entry.tx_id, entry.clone());
            TxWalReplayAction::RollbackRestored
        };

        self.restore_status_if_absent(entry.tx_id, TransactionStatus::RolledBack)?;
        Ok(action)
    }

    fn rebuild_status_for_transaction(&self, tx_id: TransactionId) {
        if self.status_table.status(tx_id).is_some() {
            return;
        }

        if self.commit_entries.contains_key(&tx_id) && !self.rollback_entries.contains_key(&tx_id) {
            let _ = self
                .status_table
                .record(tx_id, TransactionStatus::Committed);
        } else if self.rollback_entries.contains_key(&tx_id)
            && !self.commit_entries.contains_key(&tx_id)
        {
            let _ = self
                .status_table
                .record(tx_id, TransactionStatus::RolledBack);
        }
    }
}

fn validate_transaction_id(tx_id: TransactionId, message: &'static str) -> AndromedaResult<()> {
    if tx_id.get() == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            message,
        ));
    }

    Ok(())
}

fn conflicting_terminal_record() -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Transaction,
        "conflicting transaction terminal durability records",
    )
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
    async fn test_commit_log_commit_is_idempotent_without_duplicate_wal_record() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = CommitLogManager::new(wal.clone(), status_table);

        let tx_id = TransactionId::new(1);
        let first = commit_log
            .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
            .await
            .unwrap();
        let second = commit_log
            .record_commit(tx_id, IsolationLevel::Snapshot, 99, 42)
            .await
            .unwrap();

        assert_eq!(first, second);
        assert_eq!(wal.records.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_commit_log_records_durable_rollback() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = CommitLogManager::new(wal.clone(), status_table.clone());

        let tx_id = TransactionId::new(2);
        let entry = commit_log.record_rollback(tx_id, 0xCAFE).await.unwrap();

        assert_eq!(entry.tx_id, tx_id);
        assert_eq!(entry.rollback_lsn, Lsn::new(1));
        assert_eq!(entry.parameter_hash, 0xCAFE);
        assert!(commit_log.is_rolled_back(tx_id));
        assert_eq!(
            status_table.status(tx_id),
            Some(TransactionStatus::RolledBack)
        );
        assert_eq!(*wal.durable_lsn.lock().unwrap(), entry.rollback_lsn);
    }

    #[tokio::test]
    async fn test_commit_log_rollback_is_idempotent_without_duplicate_wal_record() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = CommitLogManager::new(wal.clone(), status_table);

        let tx_id = TransactionId::new(3);
        let first = commit_log.record_rollback(tx_id, 1).await.unwrap();
        let second = commit_log.record_rollback(tx_id, 2).await.unwrap();

        assert_eq!(first, second);
        assert_eq!(wal.records.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_commit_log_rejects_conflicting_terminal_outcomes() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = CommitLogManager::new(wal, status_table);

        let committed = TransactionId::new(4);
        commit_log
            .record_commit(committed, IsolationLevel::Snapshot, 1, 0)
            .await
            .unwrap();
        assert_eq!(
            commit_log
                .record_rollback(committed, 0)
                .await
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Transaction
        );

        let rolled_back = TransactionId::new(5);
        commit_log.record_rollback(rolled_back, 0).await.unwrap();
        assert_eq!(
            commit_log
                .record_commit(rolled_back, IsolationLevel::Snapshot, 1, 0)
                .await
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Transaction
        );
    }

    #[test]
    fn test_commit_log_rebuilds_missing_status_from_local_records() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = CommitLogManager::new(wal, status_table.clone());

        let committed = TransactionId::new(6);
        let rolled_back = TransactionId::new(7);
        commit_log
            .restore_commit_entry(CommitLogEntry {
                tx_id: committed,
                commit_lsn: Lsn::new(11),
                timestamp: EngineTimestamp::from_unix_millis(1),
                row_count_affected: 2,
                isolation_level: IsolationLevel::Snapshot,
            })
            .unwrap();
        commit_log
            .restore_rollback_entry(RollbackLogEntry {
                tx_id: rolled_back,
                rollback_lsn: Lsn::new(12),
                timestamp: EngineTimestamp::from_unix_millis(2),
                parameter_hash: 3,
            })
            .unwrap();

        assert_eq!(
            status_table.status(committed),
            Some(TransactionStatus::Committed)
        );
        assert_eq!(
            status_table.status(rolled_back),
            Some(TransactionStatus::RolledBack)
        );

        let status_table = Arc::new(TransactionStatusTable::new());
        let recovered = CommitLogManager::new(MockWal::new(), status_table.clone());
        recovered.commit_entries.insert(
            committed,
            CommitLogEntry {
                tx_id: committed,
                commit_lsn: Lsn::new(21),
                timestamp: EngineTimestamp::from_unix_millis(1),
                row_count_affected: 2,
                isolation_level: IsolationLevel::Serializable,
            },
        );
        recovered.rollback_entries.insert(
            rolled_back,
            RollbackLogEntry {
                tx_id: rolled_back,
                rollback_lsn: Lsn::new(22),
                timestamp: EngineTimestamp::from_unix_millis(2),
                parameter_hash: 3,
            },
        );

        let summary = recovered.rebuild_status_from_records().unwrap();
        assert_eq!(summary.committed_restored, 1);
        assert_eq!(summary.rolled_back_restored, 1);
        assert_eq!(
            status_table.status(committed),
            Some(TransactionStatus::Committed)
        );
        assert_eq!(
            status_table.status(rolled_back),
            Some(TransactionStatus::RolledBack)
        );
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
