//! Transaction-WAL Binding Adapter Trait
//!
//! # Overview
//!
//! The `TxWalAdapterTrait` provides a clean boundary between transaction management
//! and Write-Ahead Log (WAL) operations. It abstracts the contract that ties transaction
//! lifecycle events to durable WAL records, ensuring the **durable-commit doctrine**:
//!
//! > "A transaction is visible to other snapshots **only after** its commit record
//! > is durably flushed to the Write-Ahead Log."
//!
//! # Five-Step Commit Sequence
//!
//! The commit protocol implemented via this trait follows a strict order:
//!
//! 1. **State Check**: Transaction is in Committing state
//! 2. **Write to WAL**: Append TxCommit record to WAL buffer (non-blocking append)
//! 3. **Flush to Disk**: Flush WAL through the commit LSN (CRITICAL durability boundary)
//! 4. **Mark Visible**: Update TransactionStatusTable to Committed (after durability)
//! 5. **Return Durability Proof**: Return LSN and timestamp to caller
//!
//! # Contract Guarantees
//!
//! ## Commit Contract (`record_commit`)
//!
//! - **Precondition**: Transaction exists and is in Committing state
//! - **Atomicity**: Commit is atomic from caller's perspective (all-or-nothing)
//! - **Durability**: LSN returned from this call is guaranteed durable on stable storage
//! - **Visibility**: After return, the transaction is visible to new snapshots
//! - **Idempotency**: Calling twice with same tx_id returns same LSN (no double-write)
//! - **Postcondition**: `is_durably_committed(tx_id)` returns true
//!
//! ## Legacy Rollback Contract (`record_rollback`)
//!
//! This trait predates the durable rollback record used by
//! [`crate::CommitLogManager::record_rollback`] and does not receive a
//! [`WalManager`] handle in `record_rollback`, so it cannot append or flush a
//! TxRollback record by itself.
//!
//! - **Precondition**: Transaction is in Rolling Back state
//! - **Asynchronicity**: Rollback is best-effort in this legacy adapter path
//! - **Atomicity**: Rollback marks tx_id as rolled back to prevent visibility
//! - **Visibility**: Transaction never becomes visible to snapshots
//! - **Idempotency**: Multiple rollbacks for same tx_id are safe (no-op after first)
//! - **Postcondition**: `is_durably_committed(tx_id)` returns false
//!
//! New production code that needs crash-recoverable rollback evidence should use
//! `CommitLogManager::record_rollback`, which appends `TxRollback`, flushes it,
//! then marks the transaction `RolledBack`.
//!
//! ## Durability Check Contract (`is_durably_committed`)
//!
//! - **Atomicity**: Returns consistent snapshot of durability state
//! - **Monotonicity**: Once returns true for tx_id, always returns true
//! - **Queries status table** for authoritative state
//! - Returns true only if transaction is in Committed state
//!
//! ## LSN Retrieval Contract (`get_commit_lsn`)
//!
//! - **Precondition**: `tx_id` has been committed
//! - **Returns**: LSN of the durable commit record in WAL
//! - **None** if transaction was rolled back or not yet committed
//! - **Utility**: Used for recovery correlation and GC eligibility
//!
//! # Invariants
//!
//! ## Critical Invariants (Violations = Engine Bugs)
//!
//! 1. **WAL-before-visibility**: No transaction is visible before its commit record is flushed
//! 2. **Commit-idempotency**: Committing same tx_id twice returns same LSN
//! 3. **Commit-irreversibility**: Once visible, transaction never becomes invisible (no rollback)
//! 4. **Status-consistency**: StatusTable and WAL are synchronized after commit returns
//!
//! ## Recovery Invariants (Replayed in recovery)
//!
//! - Every visible transaction has a durable TxCommit record at its LSN
//! - Recovery replays all TxCommit records before opening new transactions
//! - Transactions visible in memory must be visible after recovery
//! - Rolled-back transactions never produce WAL records visible to recovery
//!
//! # Thread Safety & Concurrency
//!
//! Implementors MUST be:
//! - **Thread-safe**: Safe to share via `Arc<dyn TxWalAdapterTrait>`
//! - **Reentrant**: Concurrent calls to different tx_ids must not deadlock
//! - **Async-safe**: If using async (tokio), must not block executor
//! - **Lock-free where possible**: Use CAS, fine-grained locking, or lock-free data structures
//!
//! # Example Usage
//!
//! ```ignore
//! // Create adapter (typically singleton, held by TransactionManager)
//! let adapter: Arc<dyn TxWalAdapterTrait> = Arc::new(...)
//!
//! // Record commit during transaction completion
//! let commit_lsn = adapter.record_commit(tx_id, wal_manager)?;
//! assert!(adapter.is_durably_committed(tx_id)?);
//!
//! // Record rollback (best effort)
//! adapter.record_rollback(tx_id)?;
//! assert!(!adapter.is_durably_committed(tx_id)?);
//!
//! // Check durability for recovery correlation
//! match adapter.get_commit_lsn(tx_id)? {
//!     Some(lsn) => println!("Tx committed at LSN {:?}", lsn),
//!     None => println!("Tx not committed"),
//! }
//! ```
//!
//! # Error Handling
//!
//! Errors are categorized by `AndromedaErrorKind`:
//!
//! - **Transaction**: Transaction state is invalid or doesn't exist
//! - **Storage**: WAL I/O failed (e.g., disk full, permissions)
//! - **Internal**: Adapter invariant violated (implementation bug)
//!
//! Callers should treat Storage errors as transient and retry; Transaction errors
//! require caller to investigate the transaction state.
//!
//! # No Unsafe Code
//!
//! This trait and all implementations must be forbid(unsafe_code).

use std::{collections::BTreeMap, sync::Arc};

use crate::{IsolationLevel, Lsn, TxWalReplayRecord};
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
///
/// This trait defines the contract for recording transaction lifecycle events
/// (commits, rollbacks) to the Write-Ahead Log (WAL) and querying durability status.
///
/// # Implement this trait
///
/// When building a new transaction system, implement this trait once and reuse it
/// across all WAL-dependent transaction operations. The trait's async methods allow
/// blocking I/O without blocking the executor.
#[async_trait::async_trait]
pub trait TxWalAdapterTrait: Send + Sync {
    /// Record a transaction commit with full durability guarantee.
    ///
    /// This method implements the **five-step commit sequence** documented above:
    /// 1. Append TxCommit record to WAL buffer
    /// 2. **CRITICAL**: Flush WAL to durable storage (durability boundary)
    /// 3. Store commit metadata for fast lookup
    /// 4. Update transaction status table (after durability)
    /// 5. Return LSN proof of durability
    ///
    /// # Arguments
    ///
    /// - `tx_id`: Unique transaction identifier
    /// - `wal_manager`: Reference to WAL implementation for append/flush operations
    ///
    /// # Returns
    ///
    /// - `Ok(Lsn)`: The durable LSN of the TxCommit record in the WAL. Guaranteed
    ///   that the record at this LSN can be read during recovery.
    /// - `Err(AndromedaError)` if:
    ///   - Transaction doesn't exist (AndromedaErrorKind::Transaction)
    ///   - Transaction is not in Committing state
    ///   - WAL append fails (AndromedaErrorKind::Storage)
    ///   - WAL flush fails (AndromedaErrorKind::Storage)
    ///   - Status table update fails (AndromedaErrorKind::Internal)
    ///
    /// # Durability Guarantee
    ///
    /// After this method returns Ok(lsn), the caller can assume:
    /// - The TxCommit record is on stable storage at `lsn`
    /// - The transaction is visible to snapshots created after this call
    /// - The transaction will be visible after crash recovery
    /// - `is_durably_committed(tx_id)` will return true
    /// - `get_commit_lsn(tx_id)` will return Some(lsn)
    ///
    /// # Idempotency
    ///
    /// Calling this method twice with the same `tx_id` must return the same LSN
    /// and not create duplicate WAL records. Implementation should:
    /// - Check if tx_id is already committed (idempotent on success)
    /// - Or use a transactional status table to serialize commits
    ///
    /// # Example
    ///
    /// ```ignore
    /// let commit_lsn = adapter.record_commit(TransactionId::new(42), wal_mgr).await?;
    /// assert!(adapter.is_durably_committed(TransactionId::new(42)).await?);
    /// ```
    async fn record_commit(
        &self,
        tx_id: TransactionId,
        wal_manager: Arc<dyn WalManager>,
    ) -> AndromedaResult<Lsn>;

    /// Record a transaction rollback through the legacy best-effort adapter path.
    ///
    /// Rollbacks are best-effort operations that mark a transaction as rolled back
    /// to prevent it from becoming visible to snapshots. This trait method cannot
    /// create durable rollback evidence because it has no WAL manager argument;
    /// use [`crate::CommitLogManager::record_rollback`] for durable TxRollback
    /// append+flush semantics.
    ///
    /// # Arguments
    ///
    /// - `tx_id`: Unique transaction identifier
    ///
    /// # Returns
    ///
    /// - `Ok(())` on success
    /// - `Err(AndromedaError)` if:
    ///   - Transaction doesn't exist (AndromedaErrorKind::Transaction)
    ///   - Transaction is already committed (cannot rollback committed tx)
    ///   - Status table update fails (AndromedaErrorKind::Internal)
    ///
    /// # Legacy Asynchronicity Contract
    ///
    /// Rollback is fire-and-forget in this adapter shape. After this method returns:
    /// - The transaction is marked rolled back
    /// - No new snapshots will see the transaction
    /// - The transaction may be garbage collected
    /// - No durability is guaranteed
    ///
    /// # Idempotency
    ///
    /// Calling this method multiple times with the same `tx_id` must be safe:
    /// - First call: Marks transaction as rolled back
    /// - Subsequent calls: No-op or return Ok with no side effects
    ///
    /// # Example
    ///
    /// ```ignore
    /// adapter.record_rollback(TransactionId::new(42)).await?;
    /// assert!(!adapter.is_durably_committed(TransactionId::new(42)).await?);
    /// ```
    async fn record_rollback(&self, tx_id: TransactionId) -> AndromedaResult<()>;

    /// Check whether a transaction is durably committed.
    ///
    /// Queries the transaction status table to determine if `tx_id` is in Committed state.
    /// This is the authoritative source of visibility for snapshots.
    ///
    /// # Arguments
    ///
    /// - `tx_id`: Unique transaction identifier
    ///
    /// # Returns
    ///
    /// - `Ok(true)` if transaction is in Committed state (visible to snapshots)
    /// - `Ok(false)` if transaction is in Committing, Rolling Back, or Rolled Back state
    /// - `Err(AndromedaError)` if status table query fails
    ///
    /// # Monotonicity Property
    ///
    /// Once this function returns `Ok(true)` for a given `tx_id`, it must always
    /// return `Ok(true)` for subsequent calls (except if the entry is garbage collected,
    /// in which case false is acceptable).
    ///
    /// # Example
    ///
    /// ```ignore
    /// adapter.record_commit(tx_id, wal_mgr).await?;
    /// assert_eq!(adapter.is_durably_committed(tx_id).await?, true);
    ///
    /// adapter.record_rollback(other_tx_id).await?;
    /// assert_eq!(adapter.is_durably_committed(other_tx_id).await?, false);
    /// ```
    async fn is_durably_committed(&self, tx_id: TransactionId) -> AndromedaResult<bool>;

    /// Get the commit LSN for a transaction.
    ///
    /// Retrieves the LSN of the durable TxCommit record in the WAL. Used for:
    /// - Recovery correlation: Finding which WAL records belong to this transaction
    /// - GC eligibility: Determining if transaction is older than min_active_snapshot_lsn
    /// - Auditing: Proof of when and where the transaction committed
    ///
    /// # Arguments
    ///
    /// - `tx_id`: Unique transaction identifier
    ///
    /// # Returns
    ///
    /// - `Ok(Some(lsn))` if transaction is committed; `lsn` is the WAL record position
    /// - `Ok(None)` if transaction is not committed or was rolled back
    /// - `Err(AndromedaError)` if lookup fails
    ///
    /// # Recovery Semantics
    ///
    /// The returned LSN, if present, guarantees that:
    /// - The WAL contains a TxCommit record at exactly this LSN
    /// - This LSN is greater than all transaction data records (RowInsert, RowUpdate, etc.)
    /// - Recovery will replay all redo-relevant records before this LSN
    ///
    /// # Example
    ///
    /// ```ignore
    /// let commit_lsn = adapter.record_commit(tx_id, wal_mgr).await?;
    /// assert_eq!(adapter.get_commit_lsn(tx_id).await?, Some(commit_lsn));
    ///
    /// adapter.record_rollback(other_tx_id).await?;
    /// assert_eq!(adapter.get_commit_lsn(other_tx_id).await?, None);
    /// ```
    async fn get_commit_lsn(&self, tx_id: TransactionId) -> AndromedaResult<Option<Lsn>>;
}

/// WAL Manager interface expected by TxWalAdapterTrait.
///
/// Provides the methods needed by the adapter to record and flush WAL entries.
/// This is a subset of the full WAL API, allowing the adapter to remain decoupled
/// from the complete WAL implementation.
#[async_trait::async_trait]
pub trait WalManager: Send + Sync {
    /// Append a transaction commit record to the WAL.
    ///
    /// # Arguments
    ///
    /// - `tx_id`: Transaction identifier for the commit record
    ///
    /// # Returns
    ///
    /// - `Ok(Lsn)`: The assigned LSN for this record
    /// - `Err`: WAL append failed (disk full, I/O error, etc.)
    ///
    /// This is **non-blocking** in the sense that it adds the record to an in-memory
    /// buffer. Durability is achieved by calling `flush_through`.
    async fn append_commit(&self, tx_id: TransactionId) -> AndromedaResult<Lsn>;

    /// Flush the WAL through the specified LSN to durable storage.
    ///
    /// # Arguments
    ///
    /// - `lsn`: Target LSN. All records up to and including this LSN are flushed.
    ///
    /// # Returns
    ///
    /// - `Ok(Lsn)`: Confirms that records through this LSN are on stable storage
    /// - `Err`: Flush failed (disk full, I/O error, permissions, etc.)
    ///
    /// This is the **durability boundary**. After this returns Ok, the WAL records
    /// are guaranteed to persist across process crashes.
    ///
    /// # Important
    ///
    /// This operation is typically **BLOCKING** (synchronous I/O). Callers must
    /// ensure they don't block the executor if using async/await.
    async fn flush_through(&self, lsn: Lsn) -> AndromedaResult<Lsn>;
}

/// Append a commit record and prove that the WAL is durable through its LSN.
///
/// Callers must mark transaction visibility only after this helper returns
/// `Ok(commit_lsn)`. A successful append followed by a failed or short flush is
/// reported as a storage error and does not produce commit evidence.
pub async fn append_commit_and_flush(
    tx_id: TransactionId,
    wal_manager: Arc<dyn WalManager>,
) -> AndromedaResult<Lsn> {
    if tx_id.get() == 0 {
        return Err(TxWalAdapterError::InvalidTransactionState.into_andromeda_error());
    }

    let commit_lsn = wal_manager.append_commit(tx_id).await.map_err(|source| {
        TxWalAdapterError::WalAppendFailed.into_andromeda_error_with_source(source)
    })?;
    let durable_lsn = wal_manager
        .flush_through(commit_lsn)
        .await
        .map_err(|source| {
            TxWalAdapterError::WalFlushFailed.into_andromeda_error_with_source(source)
        })?;

    if durable_lsn < commit_lsn {
        return Err(TxWalAdapterError::DurableLsnBehindCommit.into_andromeda_error());
    }

    Ok(commit_lsn)
}

/// Error types specific to TxWalAdapterTrait failures.
///
/// New adapter implementations can use these to categorize their errors
/// before converting to `AndromedaError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxWalAdapterError {
    /// Transaction ID does not exist or is not in expected state
    InvalidTransactionState,
    /// WAL append operation failed (disk full, permissions, corruption)
    WalAppendFailed,
    /// WAL flush operation failed (I/O error, network timeout)
    WalFlushFailed,
    /// WAL reported success but did not flush through the commit LSN
    DurableLsnBehindCommit,
    /// WAL replay terminal evidence is not covered by the durable WAL prefix
    DurableLsnBehindTerminal,
    /// Status table operation failed (concurrent modification, corruption)
    StatusTableError,
    /// Internal invariant violated (implementation bug)
    InvariantViolated,
    /// Replay boundary record did not carry a transaction identifier
    MissingReplayTransactionId,
    /// Replay saw more than one TxBegin for the same transaction
    DuplicateBeginRecord,
    /// Replay saw transaction data or a terminal record without TxBegin
    ReplayRecordWithoutBegin,
    /// Replay saw both commit and rollback terminal records
    ConflictingTerminalRecord,
    /// Replay saw transaction data after commit or rollback evidence
    RecordAfterTerminal,
    /// Replay input regressed in LSN order for one transaction
    ReplayLsnRegression,
}

impl TxWalAdapterError {
    pub const fn kind(self) -> AndromedaErrorKind {
        match self {
            Self::InvalidTransactionState
            | Self::MissingReplayTransactionId
            | Self::DuplicateBeginRecord
            | Self::ReplayRecordWithoutBegin
            | Self::ConflictingTerminalRecord
            | Self::RecordAfterTerminal
            | Self::ReplayLsnRegression => AndromedaErrorKind::Transaction,
            Self::WalAppendFailed
            | Self::WalFlushFailed
            | Self::DurableLsnBehindCommit
            | Self::DurableLsnBehindTerminal => AndromedaErrorKind::Storage,
            Self::StatusTableError | Self::InvariantViolated => AndromedaErrorKind::Internal,
        }
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::InvalidTransactionState => "transaction is not in expected state for operation",
            Self::WalAppendFailed => "WAL append failed",
            Self::WalFlushFailed => "WAL flush failed",
            Self::DurableLsnBehindCommit => "durable WAL flush ended before commit LSN",
            Self::DurableLsnBehindTerminal => {
                "durable WAL replay coverage ended before terminal LSN"
            }
            Self::StatusTableError => "transaction status table error",
            Self::InvariantViolated => "TxWalAdapter invariant violation",
            Self::MissingReplayTransactionId => {
                "transaction WAL replay boundary record is missing transaction id"
            }
            Self::DuplicateBeginRecord => "transaction WAL replay saw duplicate TxBegin",
            Self::ReplayRecordWithoutBegin => "transaction WAL replay record has no TxBegin",
            Self::ConflictingTerminalRecord => {
                "transaction WAL replay saw conflicting terminal records"
            }
            Self::RecordAfterTerminal => {
                "transaction WAL replay saw record after terminal boundary"
            }
            Self::ReplayLsnRegression => "transaction WAL replay LSN order regressed",
        }
    }

    /// Convert this error to an AndromedaError with appropriate categorization.
    pub fn into_andromeda_error(self) -> AndromedaError {
        AndromedaError::new(self.kind(), self.message())
    }

    pub fn into_andromeda_error_with_source(self, source: AndromedaError) -> AndromedaError {
        AndromedaError::new(self.kind(), format!("{}: {source}", self.message()))
    }
}

impl From<TxWalAdapterError> for AndromedaError {
    fn from(error: TxWalAdapterError) -> Self {
        error.into_andromeda_error()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxWalAdapterReplayKind {
    Begin,
    Commit,
    Rollback,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxWalAdapterReplayRecord {
    pub kind: TxWalAdapterReplayKind,
    pub lsn: Lsn,
    pub durable_lsn: Lsn,
    pub tx_id: Option<TransactionId>,
    pub timestamp: andromeda_core::EngineTimestamp,
    pub row_count_affected: u64,
    pub isolation_level: IsolationLevel,
    pub parameter_hash: u64,
}

impl TxWalAdapterReplayRecord {
    pub fn begin(tx_id: TransactionId, lsn: Lsn) -> Self {
        Self::boundary(TxWalAdapterReplayKind::Begin, tx_id, lsn)
    }

    pub fn commit(
        tx_id: TransactionId,
        lsn: Lsn,
        timestamp: andromeda_core::EngineTimestamp,
    ) -> Self {
        Self {
            timestamp,
            ..Self::boundary(TxWalAdapterReplayKind::Commit, tx_id, lsn)
        }
    }

    pub fn rollback(
        tx_id: TransactionId,
        lsn: Lsn,
        timestamp: andromeda_core::EngineTimestamp,
    ) -> Self {
        Self {
            timestamp,
            ..Self::boundary(TxWalAdapterReplayKind::Rollback, tx_id, lsn)
        }
    }

    pub fn other(lsn: Lsn, tx_id: Option<TransactionId>) -> Self {
        Self {
            kind: TxWalAdapterReplayKind::Other,
            lsn,
            durable_lsn: lsn,
            tx_id,
            timestamp: andromeda_core::EngineTimestamp::ZERO,
            row_count_affected: 0,
            isolation_level: IsolationLevel::Snapshot,
            parameter_hash: 0,
        }
    }

    pub fn with_commit_metadata(
        mut self,
        row_count_affected: u64,
        isolation_level: IsolationLevel,
        parameter_hash: u64,
    ) -> Self {
        self.row_count_affected = row_count_affected;
        self.isolation_level = isolation_level;
        self.parameter_hash = parameter_hash;
        self
    }

    pub fn with_parameter_hash(mut self, parameter_hash: u64) -> Self {
        self.parameter_hash = parameter_hash;
        self
    }

    pub fn with_durable_lsn(mut self, durable_lsn: Lsn) -> Self {
        self.durable_lsn = durable_lsn;
        self
    }

    fn boundary(kind: TxWalAdapterReplayKind, tx_id: TransactionId, lsn: Lsn) -> Self {
        Self {
            kind,
            lsn,
            durable_lsn: lsn,
            tx_id: Some(tx_id),
            timestamp: andromeda_core::EngineTimestamp::ZERO,
            row_count_affected: 0,
            isolation_level: IsolationLevel::Snapshot,
            parameter_hash: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ReplayState {
    begin_lsn: Lsn,
    last_lsn: Lsn,
    terminal: Option<TxWalAdapterReplayRecord>,
}

pub fn map_tx_wal_replay_records(
    records: impl IntoIterator<Item = TxWalAdapterReplayRecord>,
) -> AndromedaResult<Vec<TxWalReplayRecord>> {
    let mut states: BTreeMap<TransactionId, ReplayState> = BTreeMap::new();
    let mut previous_record = None;

    for record in records {
        validate_global_replay_lsn_order(&mut previous_record, record)?;
        match record.kind {
            TxWalAdapterReplayKind::Begin => {
                let tx_id = replay_tx_id(record)?;
                if states.contains_key(&tx_id) {
                    return Err(tx_adapter_error(TxWalAdapterError::DuplicateBeginRecord));
                }
                states.insert(
                    tx_id,
                    ReplayState {
                        begin_lsn: record.lsn,
                        last_lsn: record.lsn,
                        terminal: None,
                    },
                );
            }
            TxWalAdapterReplayKind::Commit | TxWalAdapterReplayKind::Rollback => {
                let tx_id = replay_tx_id(record)?;
                let state = states
                    .get_mut(&tx_id)
                    .ok_or_else(|| tx_adapter_error(TxWalAdapterError::ReplayRecordWithoutBegin))?;
                validate_replay_lsn_order(state, record.lsn)?;
                validate_terminal_durable_coverage(record)?;
                if let Some(existing) = state.terminal {
                    if existing != record {
                        return Err(tx_adapter_error(
                            TxWalAdapterError::ConflictingTerminalRecord,
                        ));
                    }
                    continue;
                }
                state.terminal = Some(record);
            }
            TxWalAdapterReplayKind::Other => {
                let Some(tx_id) = record.tx_id else {
                    continue;
                };
                let state = states
                    .get_mut(&tx_id)
                    .ok_or_else(|| tx_adapter_error(TxWalAdapterError::ReplayRecordWithoutBegin))?;
                if state.terminal.is_some() {
                    return Err(tx_adapter_error(TxWalAdapterError::RecordAfterTerminal));
                }
                validate_replay_lsn_order(state, record.lsn)?;
            }
        }
    }

    let mut replay_records = Vec::with_capacity(states.len());
    for (tx_id, state) in states {
        replay_records.push(match state.terminal {
            Some(record) if record.kind == TxWalAdapterReplayKind::Commit => {
                TxWalReplayRecord::commit_with_durable_lsn(
                    tx_id,
                    record.lsn,
                    record.durable_lsn,
                    record.timestamp,
                    record.row_count_affected,
                    record.isolation_level,
                )
            }
            Some(record) if record.kind == TxWalAdapterReplayKind::Rollback => {
                TxWalReplayRecord::rollback_with_durable_lsn(
                    tx_id,
                    record.lsn,
                    record.durable_lsn,
                    record.timestamp,
                    record.parameter_hash,
                )
            }
            Some(_) => {
                return Err(tx_adapter_error(TxWalAdapterError::InvariantViolated));
            }
            None => TxWalReplayRecord::incomplete(tx_id, state.last_lsn.max(state.begin_lsn)),
        });
    }

    replay_records.sort_by_key(TxWalReplayRecord::replay_lsn);
    Ok(replay_records)
}

fn replay_tx_id(record: TxWalAdapterReplayRecord) -> AndromedaResult<TransactionId> {
    record
        .tx_id
        .ok_or_else(|| tx_adapter_error(TxWalAdapterError::MissingReplayTransactionId))
}

fn validate_replay_lsn_order(state: &mut ReplayState, lsn: Lsn) -> AndromedaResult<()> {
    if lsn < state.last_lsn {
        return Err(tx_adapter_error(TxWalAdapterError::ReplayLsnRegression));
    }
    state.last_lsn = lsn;
    Ok(())
}

fn validate_terminal_durable_coverage(record: TxWalAdapterReplayRecord) -> AndromedaResult<()> {
    if record.durable_lsn < record.lsn {
        return Err(tx_adapter_error(
            TxWalAdapterError::DurableLsnBehindTerminal,
        ));
    }

    Ok(())
}

fn validate_global_replay_lsn_order(
    previous_record: &mut Option<TxWalAdapterReplayRecord>,
    record: TxWalAdapterReplayRecord,
) -> AndromedaResult<()> {
    if let Some(previous_record) = *previous_record {
        if record.lsn < previous_record.lsn {
            return Err(tx_adapter_error(TxWalAdapterError::ReplayLsnRegression));
        }

        if record.lsn == previous_record.lsn && record != previous_record {
            return Err(tx_adapter_error(TxWalAdapterError::ReplayLsnRegression));
        }
    }

    *previous_record = Some(record);
    Ok(())
}

fn tx_adapter_error(error: TxWalAdapterError) -> AndromedaError {
    error.into_andromeda_error()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Mock WAL Manager for testing
    struct MockWalManager {
        next_lsn: Mutex<u64>,
        durable_lsn: Mutex<u64>,
    }

    impl MockWalManager {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                next_lsn: Mutex::new(1),
                durable_lsn: Mutex::new(0),
            })
        }
    }

    #[async_trait::async_trait]
    impl WalManager for MockWalManager {
        async fn append_commit(&self, _tx_id: TransactionId) -> AndromedaResult<Lsn> {
            let mut next = self
                .next_lsn
                .lock()
                .map_err(|_| TxWalAdapterError::StatusTableError.into_andromeda_error())?;
            let current = *next;
            *next = current
                .checked_add(1)
                .ok_or_else(|| TxWalAdapterError::InvariantViolated.into_andromeda_error())?;
            Ok(Lsn::new(current))
        }

        async fn flush_through(&self, lsn: Lsn) -> AndromedaResult<Lsn> {
            let mut durable = self
                .durable_lsn
                .lock()
                .map_err(|_| TxWalAdapterError::StatusTableError.into_andromeda_error())?;
            *durable = lsn.get();
            Ok(lsn)
        }
    }

    /// Mock Transaction-WAL Adapter for testing
    struct MockTxWalAdapter {
        committed_txs: Mutex<std::collections::HashMap<TransactionId, Lsn>>,
    }

    impl MockTxWalAdapter {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                committed_txs: Mutex::new(std::collections::HashMap::new()),
            })
        }
    }

    #[async_trait::async_trait]
    impl TxWalAdapterTrait for MockTxWalAdapter {
        async fn record_commit(
            &self,
            tx_id: TransactionId,
            wal_manager: Arc<dyn WalManager>,
        ) -> AndromedaResult<Lsn> {
            if tx_id.get() == 0 {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Transaction,
                    "cannot commit transaction with zero ID",
                ));
            }

            {
                let committed = self
                    .committed_txs
                    .lock()
                    .map_err(|_| TxWalAdapterError::StatusTableError.into_andromeda_error())?;
                if let Some(existing_lsn) = committed.get(&tx_id) {
                    return Ok(*existing_lsn);
                }
            }

            let lsn = append_commit_and_flush(tx_id, wal_manager).await?;

            // Record commitment
            let mut committed = self
                .committed_txs
                .lock()
                .map_err(|_| TxWalAdapterError::StatusTableError.into_andromeda_error())?;
            committed.insert(tx_id, lsn);
            Ok(lsn)
        }

        async fn record_rollback(&self, _tx_id: TransactionId) -> AndromedaResult<()> {
            // In mock, rollback is just a no-op
            Ok(())
        }

        async fn is_durably_committed(&self, tx_id: TransactionId) -> AndromedaResult<bool> {
            let committed = self
                .committed_txs
                .lock()
                .map_err(|_| TxWalAdapterError::StatusTableError.into_andromeda_error())?;
            Ok(committed.contains_key(&tx_id))
        }

        async fn get_commit_lsn(&self, tx_id: TransactionId) -> AndromedaResult<Option<Lsn>> {
            let committed = self
                .committed_txs
                .lock()
                .map_err(|_| TxWalAdapterError::StatusTableError.into_andromeda_error())?;
            Ok(committed.get(&tx_id).copied())
        }
    }

    #[tokio::test]
    async fn test_record_commit_assigns_lsn() -> AndromedaResult<()> {
        let wal = MockWalManager::new();
        let adapter = MockTxWalAdapter::new();
        let tx_id = TransactionId::new(1);

        let lsn = adapter.record_commit(tx_id, wal).await?;
        assert_eq!(lsn.get(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn test_commit_idempotency() -> AndromedaResult<()> {
        let wal = MockWalManager::new();
        let adapter = MockTxWalAdapter::new();
        let tx_id = TransactionId::new(1);

        let lsn1 = adapter.record_commit(tx_id, wal.clone()).await?;
        let lsn2 = adapter.record_commit(tx_id, wal).await?;

        assert_eq!(lsn1, lsn2);
        Ok(())
    }

    #[tokio::test]
    async fn test_is_durably_committed_after_record_commit() -> AndromedaResult<()> {
        let wal = MockWalManager::new();
        let adapter = MockTxWalAdapter::new();
        let tx_id = TransactionId::new(1);

        adapter.record_commit(tx_id, wal).await?;
        assert!(adapter.is_durably_committed(tx_id).await?);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_commit_lsn_returns_recorded_lsn() -> AndromedaResult<()> {
        let wal = MockWalManager::new();
        let adapter = MockTxWalAdapter::new();
        let tx_id = TransactionId::new(1);

        let recorded_lsn = adapter.record_commit(tx_id, wal).await?;
        let retrieved_lsn = adapter.get_commit_lsn(tx_id).await?;

        assert_eq!(retrieved_lsn, Some(recorded_lsn));
        Ok(())
    }

    #[tokio::test]
    async fn test_rollback_prevents_durability_check() -> AndromedaResult<()> {
        let _wal = MockWalManager::new();
        let adapter = MockTxWalAdapter::new();
        let tx_id = TransactionId::new(1);

        adapter.record_rollback(tx_id).await?;
        assert!(!adapter.is_durably_committed(tx_id).await?);
        Ok(())
    }

    #[tokio::test]
    async fn test_reject_zero_transaction_id() {
        let wal = MockWalManager::new();
        let adapter = MockTxWalAdapter::new();

        let result = adapter.record_commit(TransactionId::new(0), wal).await;
        assert!(matches!(
            result,
            Err(error) if error.kind() == AndromedaErrorKind::Transaction
        ));
    }

    #[test]
    fn test_wal_adapter_error_to_andromeda_error() {
        let err = TxWalAdapterError::WalFlushFailed;
        let andromeda_err = err.into_andromeda_error();
        assert_eq!(andromeda_err.kind(), AndromedaErrorKind::Storage);
    }
}
