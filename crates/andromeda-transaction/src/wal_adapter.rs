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

mod api;
mod error;
mod replay;

#[cfg(test)]
mod tests;

pub use api::{TxWalAdapterTrait, WalManager, append_commit_and_flush};
pub use error::TxWalAdapterError;
pub use replay::{TxWalAdapterReplayKind, TxWalAdapterReplayRecord, map_tx_wal_replay_records};
