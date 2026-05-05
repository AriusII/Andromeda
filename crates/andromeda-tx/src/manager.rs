//! Transaction manager that owns live state machines and the status table.
//!
//! The [`TransactionManager`] is the single production entry point for
//! beginning, committing, rolling back, and poisoning transactions. It
//! enforces the invariants encoded in [`TransactionStateMachine`] and mirrors
//! every terminal transition into the [`TransactionStatusTable`] used by MVCC
//! visibility.
//!
//! Higher-level concerns (WAL replay, MVCC snapshot construction) are
//! deliberately out of scope. Lock management is exposed only through a narrow,
//! boundary-safe coordinator/facade that validates transaction membership and
//! delegates to the lock manager without changing commit or rollback semantics.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};

use crate::allocator::TransactionIdAllocator;
use crate::lock_manager::{
    LockAcquireEvidence, LockAcquireStatus, LockManager, LockMode, LockReleaseAllEvidence,
    LockReleaseAllSummary, LockReleaseEvidence, LockResource,
};
use crate::locking_protocol::{TwoPhaseLocksValidator, TwoPhaseOperation};
use crate::mvcc_status::{TransactionStatus, TransactionStatusTable};
use crate::savepoint::{
    Savepoint, SavepointReleaseEvidence, SavepointRollbackEvidence, SavepointStack,
};
use crate::state::{TransactionState, TransactionStateMachine};

/// Snapshot of a transaction known to the manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionRecord {
    pub state_machine: TransactionStateMachine,
    pub status: TransactionStatus,
    pub savepoint_depth: usize,
}

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

/// Owner of the live transaction state machines, the status table, and the
/// id allocator.
#[derive(Debug)]
pub struct TransactionManager {
    allocator: TransactionIdAllocator,
    inner: Mutex<TransactionManagerInner>,
}

#[derive(Debug, Default)]
struct TransactionManagerInner {
    live: HashMap<TransactionId, TransactionStateMachine>,
    savepoints: HashMap<TransactionId, SavepointStack>,
    status: TransactionStatusTable,
}

impl TransactionManager {
    /// Construct a manager with a fresh allocator (next id = 1).
    pub fn new() -> Self {
        Self {
            allocator: TransactionIdAllocator::new(),
            inner: Mutex::new(TransactionManagerInner::default()),
        }
    }

    /// Construct a manager whose allocator floor is set so that the next
    /// allocated id is strictly greater than `floor`. Intended for the
    /// recovery driver.
    pub fn with_recovered_floor(floor: u64) -> Self {
        Self {
            allocator: TransactionIdAllocator::with_floor(floor),
            inner: Mutex::new(TransactionManagerInner::default()),
        }
    }

    /// Raise the allocator floor during recovery without losing live state.
    pub fn seed_allocator(&self, floor: u64) -> AndromedaResult<()> {
        self.allocator.seed(floor)
    }

    /// Begin a fresh transaction. Returns the freshly allocated id and
    /// records `InFlight` status atomically with state-machine creation.
    pub fn begin(&self) -> AndromedaResult<TransactionId> {
        let id = self.allocator.allocate();
        let mut inner = self.lock()?;
        if inner.live.contains_key(&id) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "transaction id allocator returned a live id; allocator corrupted",
            ));
        }
        let mut machine = TransactionStateMachine::new(id);
        machine.begin()?;
        inner.live.insert(id, machine);
        inner.savepoints.insert(id, SavepointStack::new());
        inner.status.record(id, TransactionStatus::InFlight)?;
        Ok(id)
    }

    /// Move an active transaction into the `Committing` state. The status
    /// table is *not* updated yet because the commit is not durable.
    pub fn request_commit(&self, id: TransactionId) -> AndromedaResult<()> {
        let mut inner = self.lock()?;
        let machine = Self::machine_mut(&mut inner.live, id)?;
        machine.request_commit()?;
        if let Some(stack) = inner.savepoints.get_mut(&id) {
            stack.clear();
        }
        Ok(())
    }

    /// Publish a durable commit. The state machine validates that the
    /// transaction was in `Committing`; the status table is mirrored to
    /// `Committed` only after the state-machine transition succeeds.
    pub fn commit_durable(&self, id: TransactionId, durable_lsn: u64) -> AndromedaResult<()> {
        let mut inner = self.lock()?;
        let machine = Self::machine_mut(&mut inner.live, id)?;
        machine.publish_visible_commit_after_durable_flush(durable_lsn)?;
        // State machine guarantees: state == Committed && durable_commit_lsn = Some(_).
        inner.status.record(id, TransactionStatus::Committed)?;
        Ok(())
    }

    /// Move an active or failed transaction into `RollingBack`. Status stays
    /// `InFlight` until the rollback is durable.
    pub fn request_rollback(&self, id: TransactionId) -> AndromedaResult<()> {
        let mut inner = self.lock()?;
        let machine = Self::machine_mut(&mut inner.live, id)?;
        machine.request_rollback()?;
        if let Some(stack) = inner.savepoints.get_mut(&id) {
            stack.clear();
        }
        Ok(())
    }

    /// Complete a rollback once the corresponding WAL record is durable.
    pub fn rollback_durable(&self, id: TransactionId, durable_lsn: u64) -> AndromedaResult<()> {
        let mut inner = self.lock()?;
        let machine = Self::machine_mut(&mut inner.live, id)?;
        machine.complete_rollback_after_durable_flush(durable_lsn)?;
        inner.status.record(id, TransactionStatus::RolledBack)?;
        Ok(())
    }

    /// Mark a transaction as poisoned. A poisoned transaction must be rolled
    /// back to reach a terminal state; the status table remains `InFlight`
    /// because no durable terminal evidence exists yet.
    pub fn poison(&self, id: TransactionId) -> AndromedaResult<()> {
        self.with_machine(id, |machine| {
            machine.apply(crate::state::TransactionEvent::Poison)
        })
    }

    /// Mark a transaction as failed (non-poison failure path).
    pub fn fail(&self, id: TransactionId) -> AndromedaResult<()> {
        self.with_machine(id, |machine| {
            machine.apply(crate::state::TransactionEvent::Fail)
        })
    }

    /// Dispose of a terminal transaction, removing its live state-machine
    /// entry. The status table retains the historical `Committed` /
    /// `RolledBack` outcome for visibility queries.
    pub fn dispose(&self, id: TransactionId) -> AndromedaResult<()> {
        let mut inner = self.lock()?;
        let machine = Self::machine_mut(&mut inner.live, id)?;
        machine.apply(crate::state::TransactionEvent::Dispose)?;
        // Only remove once Dispose succeeds (i.e. machine reached Disposed).
        debug_assert_eq!(machine.state, TransactionState::Disposed);
        inner.live.remove(&id);
        inner.savepoints.remove(&id);
        Ok(())
    }

    /// Create a transaction-local savepoint.
    ///
    /// Savepoints are permitted only while the transaction is `Active` and
    /// `InFlight`. They do not write WAL, publish MVCC visibility, or release
    /// locks. The returned marker is a local rollback contract for the storage
    /// write set.
    pub fn create_savepoint(
        &self,
        id: TransactionId,
        name: impl Into<String>,
    ) -> AndromedaResult<Savepoint> {
        let mut inner = self.lock()?;
        Self::require_active_in_flight(&inner, id, "savepoint creation")?;
        Self::savepoints_mut(&mut inner.savepoints, id)?.create(name)
    }

    /// Release the named savepoint and its nested descendants.
    ///
    /// This is metadata-only. It does not undo writes, does not release locks,
    /// and does not alter WAL/MVCC status.
    pub fn release_savepoint(
        &self,
        id: TransactionId,
        name: &str,
    ) -> AndromedaResult<SavepointReleaseEvidence> {
        let mut inner = self.lock()?;
        Self::require_active_in_flight(&inner, id, "savepoint release")?;
        Self::savepoints_mut(&mut inner.savepoints, id)?.release(name)
    }

    /// Roll back to the named savepoint and discard nested descendants.
    ///
    /// The target savepoint remains active. Callers must use the returned
    /// rollback marker to undo storage-local writes with ordinal greater than
    /// `target.rollback_marker.rollback_ordinal`; locks remain held until the
    /// outer transaction reaches a durable terminal state.
    pub fn rollback_to_savepoint(
        &self,
        id: TransactionId,
        name: &str,
    ) -> AndromedaResult<SavepointRollbackEvidence> {
        let mut inner = self.lock()?;
        Self::require_active_in_flight(&inner, id, "savepoint rollback")?;
        Self::savepoints_mut(&mut inner.savepoints, id)?.rollback_to(name)
    }

    /// Current number of active savepoints for a live transaction.
    pub fn savepoint_depth(&self, id: TransactionId) -> AndromedaResult<usize> {
        let inner = self.lock()?;
        Self::require_known_transaction_in_inner(&inner, id)?;
        Ok(inner.savepoints.get(&id).map_or(0, SavepointStack::depth))
    }

    /// Inspect a transaction's current state-machine and mirrored status.
    pub fn snapshot(&self, id: TransactionId) -> AndromedaResult<Option<TransactionRecord>> {
        let inner = self.lock()?;
        let Some(machine) = inner.live.get(&id).copied() else {
            return Ok(None);
        };
        let Some(status) = inner.status.status(id) else {
            return Ok(None);
        };
        Ok(Some(TransactionRecord {
            state_machine: machine,
            status,
            savepoint_depth: inner.savepoints.get(&id).map_or(0, SavepointStack::depth),
        }))
    }

    /// Read-only access to the underlying status table snapshot.
    pub fn status(&self, id: TransactionId) -> AndromedaResult<Option<TransactionStatus>> {
        Ok(self.lock()?.status.status(id))
    }

    /// Borrow the allocator for callers that need to peek (e.g. recovery
    /// instrumentation). The allocator is internally synchronized.
    pub fn allocator(&self) -> &TransactionIdAllocator {
        &self.allocator
    }

    /// Build a boundary-safe lock coordinator over this transaction manager and
    /// the supplied lock manager.
    pub const fn lock_coordinator<'a>(
        &'a self,
        locks: &'a LockManager,
    ) -> TransactionLockCoordinator<'a> {
        TransactionLockCoordinator::new(self, locks)
    }

    /// Facade helper for [`TransactionLockCoordinator::acquire`].
    pub fn acquire_lock(
        &self,
        locks: &LockManager,
        tx_id: TransactionId,
        resource: LockResource,
        mode: LockMode,
    ) -> AndromedaResult<LockAcquireStatus> {
        self.lock_coordinator(locks).acquire(tx_id, resource, mode)
    }

    /// Facade helper for [`TransactionLockCoordinator::acquire_with_evidence`].
    pub fn acquire_lock_with_evidence(
        &self,
        locks: &LockManager,
        tx_id: TransactionId,
        resource: LockResource,
        mode: LockMode,
    ) -> AndromedaResult<LockAcquireEvidence> {
        self.lock_coordinator(locks)
            .acquire_with_evidence(tx_id, resource, mode)
    }

    /// Facade helper for [`TransactionLockCoordinator::release`].
    pub fn release_lock(
        &self,
        locks: &LockManager,
        tx_id: TransactionId,
        resource: LockResource,
    ) -> AndromedaResult<bool> {
        self.lock_coordinator(locks).release(tx_id, resource)
    }

    /// Facade helper for [`TransactionLockCoordinator::release_with_evidence`].
    pub fn release_lock_with_evidence(
        &self,
        locks: &LockManager,
        tx_id: TransactionId,
        resource: LockResource,
    ) -> AndromedaResult<LockReleaseEvidence> {
        self.lock_coordinator(locks)
            .release_with_evidence(tx_id, resource)
    }

    /// Facade helper for [`TransactionLockCoordinator::release_all`].
    pub fn release_all_locks(
        &self,
        locks: &LockManager,
        tx_id: TransactionId,
    ) -> AndromedaResult<LockReleaseAllSummary> {
        self.lock_coordinator(locks).release_all(tx_id)
    }

    /// Facade helper for
    /// [`TransactionLockCoordinator::release_all_with_evidence`].
    pub fn release_all_locks_with_evidence(
        &self,
        locks: &LockManager,
        tx_id: TransactionId,
    ) -> AndromedaResult<LockReleaseAllEvidence> {
        self.lock_coordinator(locks)
            .release_all_with_evidence(tx_id)
    }

    /// Number of live (non-disposed) transactions.
    pub fn live_count(&self) -> AndromedaResult<usize> {
        Ok(self.lock()?.live.len())
    }

    fn with_machine<F>(&self, id: TransactionId, f: F) -> AndromedaResult<()>
    where
        F: FnOnce(&mut TransactionStateMachine) -> AndromedaResult<()>,
    {
        let mut inner = self.lock()?;
        let machine = Self::machine_mut(&mut inner.live, id)?;
        f(machine)
    }

    fn machine_mut(
        live: &mut HashMap<TransactionId, TransactionStateMachine>,
        id: TransactionId,
    ) -> AndromedaResult<&mut TransactionStateMachine> {
        live.get_mut(&id).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "transaction id is not registered with the manager",
            )
        })
    }

    fn require_lock_acquire_transaction(&self, id: TransactionId) -> AndromedaResult<()> {
        Self::validate_non_zero_transaction_id(id)?;

        let inner = self.lock()?;
        let machine = inner.live.get(&id).ok_or_else(Self::unknown_transaction)?;
        let status = inner
            .status
            .status(id)
            .ok_or_else(Self::unknown_transaction)?;

        if machine.state != TransactionState::Active || status != TransactionStatus::InFlight {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "lock acquisition requires an active in-flight transaction",
            ));
        }

        // 2PL validation: only Active state allows lock acquisition (growing phase)
        TwoPhaseLocksValidator::validate_operation(machine.state, TwoPhaseOperation::Acquire)?;

        Ok(())
    }

    fn require_known_transaction(&self, id: TransactionId) -> AndromedaResult<()> {
        Self::validate_non_zero_transaction_id(id)?;

        if self.lock()?.status.status(id).is_none() {
            return Err(Self::unknown_transaction());
        }

        Ok(())
    }

    fn require_known_transaction_in_inner(
        inner: &TransactionManagerInner,
        id: TransactionId,
    ) -> AndromedaResult<()> {
        Self::validate_non_zero_transaction_id(id)?;

        if inner.status.status(id).is_none() {
            return Err(Self::unknown_transaction());
        }

        Ok(())
    }

    fn require_active_in_flight(
        inner: &TransactionManagerInner,
        id: TransactionId,
        operation: &'static str,
    ) -> AndromedaResult<()> {
        Self::validate_non_zero_transaction_id(id)?;
        let machine = inner.live.get(&id).ok_or_else(Self::unknown_transaction)?;
        let status = inner
            .status
            .status(id)
            .ok_or_else(Self::unknown_transaction)?;

        if machine.state != TransactionState::Active || status != TransactionStatus::InFlight {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                format!("{operation} requires an active in-flight transaction"),
            ));
        }

        Ok(())
    }

    fn savepoints_mut(
        savepoints: &mut HashMap<TransactionId, SavepointStack>,
        id: TransactionId,
    ) -> AndromedaResult<&mut SavepointStack> {
        savepoints
            .get_mut(&id)
            .ok_or_else(Self::unknown_transaction)
    }

    fn require_terminal_cleanup_transaction(&self, id: TransactionId) -> AndromedaResult<()> {
        Self::validate_non_zero_transaction_id(id)?;

        match self.lock()?.status.status(id) {
            Some(TransactionStatus::Committed | TransactionStatus::RolledBack) => Ok(()),
            Some(TransactionStatus::InFlight) => Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "release_all locks requires committed or rolled-back transaction evidence",
            )),
            None => Err(Self::unknown_transaction()),
        }
    }

    fn validate_non_zero_transaction_id(id: TransactionId) -> AndromedaResult<()> {
        if id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "transaction id must not be zero",
            ));
        }

        Ok(())
    }

    fn unknown_transaction() -> AndromedaError {
        AndromedaError::new(
            AndromedaErrorKind::Transaction,
            "transaction id is not registered with the manager",
        )
    }

    fn lock(&self) -> AndromedaResult<MutexGuard<'_, TransactionManagerInner>> {
        self.inner.lock().map_err(|_| {
            AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "transaction manager mutex was poisoned",
            )
        })
    }
}

impl Default for TransactionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TransactionState;

    #[test]
    fn begin_allocates_unique_monotonic_ids_and_mirrors_in_flight() {
        let mgr = TransactionManager::new();
        let a = mgr.begin().unwrap();
        let b = mgr.begin().unwrap();
        let c = mgr.begin().unwrap();
        assert!(a.get() < b.get() && b.get() < c.get());
        for id in [a, b, c] {
            assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::InFlight));
            let snap = mgr.snapshot(id).unwrap().unwrap();
            assert_eq!(snap.state_machine.state, TransactionState::Active);
            assert_eq!(snap.status, TransactionStatus::InFlight);
            assert_eq!(snap.savepoint_depth, 0);
        }
    }

    #[test]
    fn savepoint_stack_is_transaction_local_and_nested() {
        let mgr = TransactionManager::new();
        let id = mgr.begin().unwrap();

        let outer = mgr.create_savepoint(id, "outer").unwrap();
        let inner = mgr.create_savepoint(id, "inner").unwrap();
        assert_eq!(outer.id.get(), 1);
        assert_eq!(inner.id.get(), 2);
        assert_eq!(mgr.savepoint_depth(id).unwrap(), 2);
        assert_eq!(mgr.snapshot(id).unwrap().unwrap().savepoint_depth, 2);

        let rollback = mgr.rollback_to_savepoint(id, "outer").unwrap();
        assert_eq!(rollback.target.name.as_str(), "outer");
        assert_eq!(rollback.discarded_descendants.len(), 1);
        assert_eq!(rollback.discarded_descendants[0].name.as_str(), "inner");
        assert_eq!(mgr.savepoint_depth(id).unwrap(), 1);

        let release = mgr.release_savepoint(id, "outer").unwrap();
        assert_eq!(release.released.len(), 1);
        assert_eq!(release.released[0].name.as_str(), "outer");
        assert_eq!(mgr.savepoint_depth(id).unwrap(), 0);
    }

    #[test]
    fn savepoint_names_are_unique_within_transaction_but_not_across_transactions() {
        let mgr = TransactionManager::new();
        let a = mgr.begin().unwrap();
        let b = mgr.begin().unwrap();

        mgr.create_savepoint(a, "sp").unwrap();
        assert_eq!(
            mgr.create_savepoint(a, "sp").unwrap_err().kind(),
            AndromedaErrorKind::Transaction
        );
        mgr.create_savepoint(b, "sp").unwrap();
    }

    #[test]
    fn savepoint_operations_require_active_in_flight_transaction() {
        let mgr = TransactionManager::new();
        let id = mgr.begin().unwrap();
        mgr.create_savepoint(id, "sp").unwrap();

        mgr.request_commit(id).unwrap();
        assert_eq!(mgr.savepoint_depth(id).unwrap(), 0);
        assert_eq!(
            mgr.create_savepoint(id, "late").unwrap_err().kind(),
            AndromedaErrorKind::Transaction
        );
        assert_eq!(
            mgr.rollback_to_savepoint(id, "sp").unwrap_err().kind(),
            AndromedaErrorKind::Transaction
        );
        assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::InFlight));
    }

    #[test]
    fn full_rollback_clears_savepoints_before_durable_terminal_status() {
        let mgr = TransactionManager::new();
        let id = mgr.begin().unwrap();
        mgr.create_savepoint(id, "sp").unwrap();

        mgr.request_rollback(id).unwrap();
        assert_eq!(mgr.savepoint_depth(id).unwrap(), 0);
        assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::InFlight));
        mgr.rollback_durable(id, 11).unwrap();
        assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::RolledBack));
    }

    #[test]
    fn commit_path_requires_durable_lsn_before_status_mirrors_committed() {
        let mgr = TransactionManager::new();
        let id = mgr.begin().unwrap();
        mgr.request_commit(id).unwrap();

        // Status is still InFlight until durable evidence arrives.
        assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::InFlight));

        // A zero LSN is rejected by the underlying state machine.
        let err = mgr.commit_durable(id, 0).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
        assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::InFlight));

        mgr.commit_durable(id, 42).unwrap();
        assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::Committed));

        let snap = mgr.snapshot(id).unwrap().unwrap();
        assert!(snap.state_machine.is_visible_committed());
        assert_eq!(snap.state_machine.durable_commit_lsn, Some(42));
    }

    #[test]
    fn rollback_path_requires_durable_lsn_before_status_mirrors_rolled_back() {
        let mgr = TransactionManager::new();
        let id = mgr.begin().unwrap();
        mgr.request_rollback(id).unwrap();
        assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::InFlight));
        mgr.rollback_durable(id, 7).unwrap();
        assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::RolledBack));
    }

    #[test]
    fn poison_then_rollback_durable_marks_status_rolled_back() {
        let mgr = TransactionManager::new();
        let id = mgr.begin().unwrap();
        mgr.poison(id).unwrap();
        assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::InFlight));
        mgr.request_rollback(id).unwrap();
        mgr.rollback_durable(id, 9).unwrap();
        assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::RolledBack));
    }

    #[test]
    fn unknown_transaction_is_rejected() {
        let mgr = TransactionManager::new();
        let stranger = TransactionId::new(9_999);
        assert_eq!(
            mgr.commit_durable(stranger, 1).unwrap_err().kind(),
            AndromedaErrorKind::Transaction
        );
        assert_eq!(
            mgr.request_rollback(stranger).unwrap_err().kind(),
            AndromedaErrorKind::Transaction
        );
        assert_eq!(
            mgr.poison(stranger).unwrap_err().kind(),
            AndromedaErrorKind::Transaction
        );
    }

    #[test]
    fn poisoned_manager_mutex_is_reported_as_transaction_error() {
        let mgr = TransactionManager::new();
        let id = mgr.begin().unwrap();

        let panic_result = std::panic::catch_unwind(|| {
            let _guard = mgr.inner.lock().unwrap();
            panic!("intentional poison for transaction manager mutex test");
        });
        assert!(panic_result.is_err());

        let err = mgr.status(id).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
        assert_eq!(err.message(), "transaction manager mutex was poisoned");
    }

    #[test]
    fn dispose_removes_committed_transaction_but_keeps_status_history() {
        let mgr = TransactionManager::new();
        let id = mgr.begin().unwrap();
        mgr.request_commit(id).unwrap();
        mgr.commit_durable(id, 5).unwrap();
        assert_eq!(mgr.live_count().unwrap(), 1);

        mgr.dispose(id).unwrap();
        assert_eq!(mgr.live_count().unwrap(), 0);
        // Status table preserves the historical outcome for visibility.
        assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::Committed));
        assert!(mgr.snapshot(id).unwrap().is_none());
    }

    #[test]
    fn recovery_seed_prevents_id_reuse() {
        let mgr = TransactionManager::with_recovered_floor(1_000);
        let id = mgr.begin().unwrap();
        assert!(id.get() > 1_000);

        // Seeding to a lower floor must be rejected.
        let err = mgr.seed_allocator(10).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Transaction);

        // Seeding to a higher floor lifts the next allocation.
        mgr.seed_allocator(5_000).unwrap();
        let next = mgr.begin().unwrap();
        assert!(next.get() > 5_000);
    }

    #[test]
    fn cannot_commit_without_request_commit_first() {
        let mgr = TransactionManager::new();
        let id = mgr.begin().unwrap();
        // Skipping `request_commit` should be rejected by the state machine.
        let err = mgr.commit_durable(id, 1).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
        assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::InFlight));
    }

    #[test]
    fn status_does_not_mirror_terminal_state_when_state_machine_rejects() {
        // If durable_commit_lsn is zero, the state machine refuses; status
        // table must remain InFlight (no half-mirrored terminal state).
        let mgr = TransactionManager::new();
        let id = mgr.begin().unwrap();
        mgr.request_commit(id).unwrap();
        assert!(mgr.commit_durable(id, 0).is_err());
        assert_eq!(mgr.status(id).unwrap(), Some(TransactionStatus::InFlight));
    }
}
