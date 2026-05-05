//! Commit protocol orchestrating the five-step transaction commit sequence.
//!
//! This module implements the `CommitProtocol` which coordinates transaction
//! state machine transitions with WAL durability and MVCC visibility updates.
//!
//! # Five-Step Commit Sequence
//!
//! The protocol enforces a strict ordering:
//!
//! 1. **State Check**: Validate transaction is in Committing state
//! 2. **Write to WAL**: Append TxCommit record (non-blocking)
//! 3. **Flush to Disk**: Durability barrier (blocking)
//! 4. **Mark Visible**: Update transaction status (after durability)
//! 5. **Clean Up**: Dispose transaction and emit traces
//!
//! # Invariants
//!
//! - **Durability First**: No visibility without durable WAL
//! - **State Machine Respected**: Only Committing → Committed transitions
//! - **Transaction Isolation**: Multiple commit protocols can run concurrently
//!   without interference (thanks to CommitLogManager's thread safety)

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use std::sync::Arc;

use crate::commit_log::{CommitLogManager, IsolationLevel};
use crate::state::TransactionState;

/// Coordinator for the transaction commit protocol.
///
/// Combines transaction state machine validation with WAL durability
/// and MVCC visibility management. Can be instantiated per-commit or
/// reused across multiple commits safely.
pub struct CommitProtocol {
    /// Reference to commit log manager for durability and visibility
    commit_log: Arc<CommitLogManager>,
}

impl CommitProtocol {
    /// Create a new commit protocol with dependency injection.
    pub fn new(commit_log: Arc<CommitLogManager>) -> Self {
        Self { commit_log }
    }

    /// Execute the complete commit protocol for a transaction.
    ///
    /// This method implements the authoritative five-step commit sequence,
    /// ensuring that:
    /// 1. The transaction is in a valid state for commitment
    /// 2. The commit is durably recorded in the WAL
    /// 3. The transaction becomes visible to new snapshots only after durability
    /// 4. Transaction state transitions are atomic
    ///
    /// # Arguments
    ///
    /// - `tx_id`: Unique transaction identifier
    /// - `current_state`: Current state of the transaction
    /// - `isolation_level`: Isolation level at which the transaction executed
    /// - `affected_rows`: Count of rows modified by the transaction
    ///
    /// # Returns
    ///
    /// Ok(()) if the commit succeeds (transaction is now durable and visible).
    /// Err if the transaction is not in Committing state or WAL fails.
    ///
    /// # State Transitions
    ///
    /// On success, the transaction transitions from Committing → Committed.
    /// The caller is responsible for subsequent Committed → Disposed transition.
    pub async fn execute_commit(
        &self,
        tx_id: TransactionId,
        current_state: TransactionState,
        isolation_level: IsolationLevel,
        affected_rows: u64,
    ) -> AndromedaResult<()> {
        // Phase 1: State Validation
        // Ensure transaction is ready to commit (must be in Committing state)
        if current_state != TransactionState::Committing {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                format!(
                    "Cannot commit transaction in state {:?}; expected Committing",
                    current_state
                ),
            ));
        }

        // Phase 2-4: Delegate to CommitLogManager
        // This performs:
        // - Write to WAL
        // - Flush to disk (durability boundary)
        // - Mark visible in status table
        // All three happen atomically within record_commit
        self.commit_log
            .record_commit(tx_id, isolation_level, affected_rows, 0)
            .await?;

        // Phase 5: Emit commit trace for audit and forensics
        // (Trace emission is internal to commit_log.record_commit)

        Ok(())
    }

    /// Verify that a transaction commit is durable after recovery.
    ///
    /// Used during crash recovery to validate that all transactions
    /// visible in the status table have corresponding WAL records.
    pub fn verify_durability(&self, tx_id: TransactionId) -> AndromedaResult<()> {
        self.commit_log.verify_durability(tx_id)
    }

    /// Get the commit LSN for a transaction (if committed).
    pub fn get_commit_lsn(
        &self,
        tx_id: TransactionId,
    ) -> Option<andromeda_storage::Lsn> {
        self.commit_log.get_commit_lsn(tx_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit_log::{CommitLogManager, InvocationWal};
    use andromeda_storage::{Lsn, WalRecordKind};
    use andromeda_core::TransactionId;

    struct TestWal {
        records: std::sync::Mutex<Vec<Lsn>>,
    }

    impl TestWal {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                records: std::sync::Mutex::new(Vec::new()),
            })
        }
    }

    #[async_trait::async_trait]
    impl InvocationWal for TestWal {
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
            Ok(lsn)
        }
    }

    #[tokio::test]
    async fn test_protocol_executes_commit_in_committing_state() {
        let wal = TestWal::new();
        let status_table = Arc::new(crate::mvcc_status::TransactionStatusTable::new());
        let commit_log = Arc::new(CommitLogManager::new(wal, status_table));
        let protocol = CommitProtocol::new(commit_log.clone());

        let tx_id = TransactionId::new(1);
        let result = protocol
            .execute_commit(
                tx_id,
                TransactionState::Committing,
                IsolationLevel::Snapshot,
                10,
            )
            .await;

        assert!(result.is_ok());
        assert!(commit_log.is_committed(tx_id));
    }

    #[tokio::test]
    async fn test_protocol_rejects_commit_in_active_state() {
        let wal = TestWal::new();
        let status_table = Arc::new(crate::mvcc_status::TransactionStatusTable::new());
        let commit_log = Arc::new(CommitLogManager::new(wal, status_table));
        let protocol = CommitProtocol::new(commit_log);

        let result = protocol
            .execute_commit(
                TransactionId::new(1),
                TransactionState::Active,
                IsolationLevel::Snapshot,
                10,
            )
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), AndromedaErrorKind::Transaction);
    }

    #[tokio::test]
    async fn test_protocol_rejects_commit_in_created_state() {
        let wal = TestWal::new();
        let status_table = Arc::new(crate::mvcc_status::TransactionStatusTable::new());
        let commit_log = Arc::new(CommitLogManager::new(wal, status_table));
        let protocol = CommitProtocol::new(commit_log);

        let result = protocol
            .execute_commit(
                TransactionId::new(1),
                TransactionState::Created,
                IsolationLevel::Snapshot,
                10,
            )
            .await;

        assert!(result.is_err());
    }
}
