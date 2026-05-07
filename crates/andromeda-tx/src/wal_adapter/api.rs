use std::sync::Arc;

use andromeda_core::{AndromedaResult, TransactionId};

use crate::Lsn;

use super::error::TxWalAdapterError;

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
