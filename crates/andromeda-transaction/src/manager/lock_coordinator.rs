use andromeda_error::AndromedaResult;
use andromeda_locking::{
    LockAcquireEvidence, LockAcquireStatus, LockManager, LockMode, LockReleaseAllEvidence,
    LockReleaseAllSummary, LockReleaseEvidence, LockResource,
};
use andromeda_types::TransactionId;

use super::manager_core::TransactionManager;

/// Boundary-safe lock facade tied to a transaction manager.
///
/// The coordinator validates transaction ids against the transaction manager
/// before delegating to [`LockManager`]. It does not mutate transaction state,
/// does not make durability claims, and does not publish commit/rollback
/// visibility. [`Self::release_all`] is terminal cleanup only: callers must
/// already have commit or rollback evidence modeled in [`TransactionManager`]
/// (`Committed` or `RolledBack` status).
#[derive(Debug, Clone, Copy)]
pub struct TransactionLockCoordinator<'a> {
    transactions: &'a TransactionManager,
    locks: &'a LockManager,
}

impl<'a> TransactionLockCoordinator<'a> {
    pub const fn new(transactions: &'a TransactionManager, locks: &'a LockManager) -> Self {
        Self {
            transactions,
            locks,
        }
    }

    /// Acquire a lock for a live, in-flight transaction.
    ///
    /// # 2PL Enforcement
    /// Lock acquisition is strictly permitted only in the **Growing Phase** (Active state).
    /// This method validates that the transaction is in Active state before delegating to the
    /// lock manager. Once a transaction enters Committing or RollingBack states (shrinking phase),
    /// no new lock acquisitions are permitted to maintain serializability.
    ///
    /// # Returns
    /// This returns the lock manager's nonblocking decision unchanged
    /// ([`LockAcquireStatus::Granted`], waiting evidence, re-entry, or upgrade
    /// status) and never changes the transaction state machine.
    ///
    /// # Errors
    /// Returns `AndromedaError` with `AndromedaErrorKind::Transaction` if:
    /// - The transaction is not registered with the manager
    /// - The transaction is not in Active state (2PL violation)
    /// - The transaction status is not InFlight
    pub fn acquire(
        &self,
        tx_id: TransactionId,
        resource: LockResource,
        mode: LockMode,
    ) -> AndromedaResult<LockAcquireStatus> {
        self.transactions.require_lock_acquire_transaction(tx_id)?;
        self.locks.acquire(tx_id, resource, mode)
    }

    /// Acquire a lock and return local trace evidence for critical waits.
    ///
    /// The evidence is observational only and never changes transaction state or
    /// durability state.
    pub fn acquire_with_evidence(
        &self,
        tx_id: TransactionId,
        resource: LockResource,
        mode: LockMode,
    ) -> AndromedaResult<LockAcquireEvidence> {
        self.transactions.require_lock_acquire_transaction(tx_id)?;
        self.locks.acquire_with_evidence(tx_id, resource, mode)
    }

    /// Release this transaction's records for a single resource.
    ///
    /// # 2PL Enforcement
    /// Lock release is permitted only in the **Shrinking Phase** (Committing or RollingBack states).
    /// Once any lock is released, the transaction enters the shrinking phase and may not acquire
    /// additional locks (strict 2PL). This coordinator does not enforce state transitions but
    /// documents the intended protocol.
    ///
    /// # Semantics
    /// This is a lock-table operation only. It does not imply abort, rollback,
    /// commit, durability, or MVCC visibility.
    pub fn release(&self, tx_id: TransactionId, resource: LockResource) -> AndromedaResult<bool> {
        self.transactions.require_known_transaction(tx_id)?;
        self.locks.release(tx_id, resource)
    }

    /// Release one resource and return local promotion trace evidence.
    ///
    /// This remains a lock-table operation only and does not imply abort,
    /// rollback, commit, or durability.
    pub fn release_with_evidence(
        &self,
        tx_id: TransactionId,
        resource: LockResource,
    ) -> AndromedaResult<LockReleaseEvidence> {
        self.transactions.require_known_transaction(tx_id)?;
        self.locks.release_with_evidence(tx_id, resource)
    }

    /// Remove all lock records for a transaction after terminal evidence.
    ///
    /// # 2PL Enforcement
    /// This method implements the **Terminal Cleanup** phase of 2PL. It is only called after
    /// the transaction has reached a terminal state (Committed or RolledBack) with durable
    /// evidence. All lock records are removed atomically, completing the 2PL protocol.
    ///
    /// # Preconditions
    /// The transaction manager must already have recorded `Committed` or `RolledBack` status.
    ///
    /// # Returns
    /// The returned [`LockReleaseAllSummary`] is cleanup evidence only and must not be
    /// interpreted as WAL/durability evidence.
    ///
    /// # Errors
    /// Returns `AndromedaError` if the transaction is not in a terminal state (Committed/RolledBack).
    pub fn release_all(&self, tx_id: TransactionId) -> AndromedaResult<LockReleaseAllSummary> {
        self.transactions
            .require_terminal_cleanup_transaction(tx_id)?;
        self.locks.release_all(tx_id)
    }

    /// Remove all lock records after terminal evidence and return local trace
    /// evidence for the cleanup plus any promotions.
    pub fn release_all_with_evidence(
        &self,
        tx_id: TransactionId,
    ) -> AndromedaResult<LockReleaseAllEvidence> {
        self.transactions
            .require_terminal_cleanup_transaction(tx_id)?;
        self.locks.release_all_with_evidence(tx_id)
    }
}
