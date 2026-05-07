use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};

use crate::allocator::TransactionIdAllocator;
use crate::lock_manager::{
    LockAcquireEvidence, LockAcquireStatus, LockManager, LockMode, LockReleaseAllEvidence,
    LockReleaseAllSummary, LockReleaseEvidence, LockResource,
};
use crate::locking_protocol::{TwoPhaseLocksValidator, TwoPhaseOperation};
use crate::lsn::Lsn;
use crate::mvcc_status::{TransactionStatus, TransactionStatusTable};
use crate::savepoint::{
    Savepoint, SavepointReleaseEvidence, SavepointRollbackEvidence, SavepointStack,
};
use crate::state::{TransactionState, TransactionStateMachine};

use super::lock_coordinator::TransactionLockCoordinator;
use super::record::TransactionRecord;

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
        let id = self.allocator.allocate()?;
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
        self.commit_durable_after_wal_record(id, durable_lsn, durable_lsn)
    }

    /// Publish a durable commit with explicit terminal WAL evidence.
    ///
    /// `commit_record_lsn` is the TxCommit record position and `durable_lsn`
    /// is the durable WAL prefix reported by storage. Visibility is mirrored
    /// only when `durable_lsn >= commit_record_lsn`.
    pub fn commit_durable_after_wal_record(
        &self,
        id: TransactionId,
        commit_record_lsn: u64,
        durable_lsn: u64,
    ) -> AndromedaResult<()> {
        let mut inner = self.lock()?;
        let machine = Self::machine_mut(&mut inner.live, id)?;
        machine.publish_visible_commit_with_durable_evidence(commit_record_lsn, durable_lsn)?;
        // State machine guarantees: state == Committed && durable_commit_lsn = Some(_).
        inner.status.record_committed_after_durable_wal(
            id,
            Lsn::new(commit_record_lsn),
            Lsn::new(durable_lsn),
        )?;
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
        self.rollback_durable_after_wal_record(id, durable_lsn, durable_lsn)
    }

    /// Complete a rollback with explicit terminal WAL evidence.
    ///
    /// `rollback_record_lsn` is the TxRollback record position and
    /// `durable_lsn` is the durable WAL prefix reported by storage. The
    /// rollback status is mirrored only when the durable prefix covers the
    /// terminal record.
    pub fn rollback_durable_after_wal_record(
        &self,
        id: TransactionId,
        rollback_record_lsn: u64,
        durable_lsn: u64,
    ) -> AndromedaResult<()> {
        let mut inner = self.lock()?;
        let machine = Self::machine_mut(&mut inner.live, id)?;
        machine.complete_rollback_with_durable_evidence(rollback_record_lsn, durable_lsn)?;
        inner.status.record_rolled_back_after_durable_wal(
            id,
            Lsn::new(rollback_record_lsn),
            Lsn::new(durable_lsn),
        )?;
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
        if machine.state() != TransactionState::Disposed {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "transaction dispose did not reach Disposed state",
            ));
        }
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

    pub(super) fn require_lock_acquire_transaction(
        &self,
        id: TransactionId,
    ) -> AndromedaResult<()> {
        Self::validate_non_zero_transaction_id(id)?;

        let inner = self.lock()?;
        let machine = inner.live.get(&id).ok_or_else(Self::unknown_transaction)?;
        let status = inner
            .status
            .status(id)
            .ok_or_else(Self::unknown_transaction)?;

        if machine.state() != TransactionState::Active || status != TransactionStatus::InFlight {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "lock acquisition requires an active in-flight transaction",
            ));
        }

        // 2PL validation: only Active state allows lock acquisition (growing phase)
        TwoPhaseLocksValidator::validate_operation(machine.state(), TwoPhaseOperation::Acquire)?;

        Ok(())
    }

    pub(super) fn require_known_transaction(&self, id: TransactionId) -> AndromedaResult<()> {
        Self::validate_non_zero_transaction_id(id)?;

        let inner = self.lock()?;
        let machine = inner.live.get(&id).ok_or_else(Self::unknown_transaction)?;
        let status = inner
            .status
            .status(id)
            .ok_or_else(Self::unknown_transaction)?;

        if status != TransactionStatus::InFlight {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "single-resource lock release requires an in-flight shrinking transaction",
            ));
        }

        TwoPhaseLocksValidator::validate_operation(machine.state(), TwoPhaseOperation::Release)?;

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

        if machine.state() != TransactionState::Active || status != TransactionStatus::InFlight {
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

    pub(super) fn require_terminal_cleanup_transaction(
        &self,
        id: TransactionId,
    ) -> AndromedaResult<()> {
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
#[path = "tests.rs"]
mod tests;
