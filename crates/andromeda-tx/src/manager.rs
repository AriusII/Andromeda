//! Transaction manager that owns live state machines and the status table.
//!
//! The [`TransactionManager`] is the single production entry point for
//! beginning, committing, rolling back, and poisoning transactions. It
//! enforces the invariants encoded in [`TransactionStateMachine`] and mirrors
//! every terminal transition into the [`TransactionStatusTable`] used by MVCC
//! visibility.
//!
//! Higher-level concerns (WAL replay, MVCC snapshot construction, lock
//! management) are deliberately out of scope. They consume the transaction
//! manager through its narrow API.

use std::collections::HashMap;
use std::sync::Mutex;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};

use crate::allocator::TransactionIdAllocator;
use crate::mvcc_status::{TransactionStatus, TransactionStatusTable};
use crate::state::{TransactionState, TransactionStateMachine};

/// Snapshot of a transaction known to the manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionRecord {
    pub state_machine: TransactionStateMachine,
    pub status: TransactionStatus,
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
        let mut inner = self.lock();
        if inner.live.contains_key(&id) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "transaction id allocator returned a live id; allocator corrupted",
            ));
        }
        let mut machine = TransactionStateMachine::new(id);
        machine.begin()?;
        inner.live.insert(id, machine);
        inner.status.record(id, TransactionStatus::InFlight)?;
        Ok(id)
    }

    /// Move an active transaction into the `Committing` state. The status
    /// table is *not* updated yet because the commit is not durable.
    pub fn request_commit(&self, id: TransactionId) -> AndromedaResult<()> {
        self.with_machine(id, |machine| machine.request_commit())
    }

    /// Publish a durable commit. The state machine validates that the
    /// transaction was in `Committing`; the status table is mirrored to
    /// `Committed` only after the state-machine transition succeeds.
    pub fn commit_durable(&self, id: TransactionId, durable_lsn: u64) -> AndromedaResult<()> {
        let mut inner = self.lock();
        let machine = Self::machine_mut(&mut inner.live, id)?;
        machine.publish_visible_commit_after_durable_flush(durable_lsn)?;
        // State machine guarantees: state == Committed && durable_commit_lsn = Some(_).
        inner.status.record(id, TransactionStatus::Committed)?;
        Ok(())
    }

    /// Move an active or failed transaction into `RollingBack`. Status stays
    /// `InFlight` until the rollback is durable.
    pub fn request_rollback(&self, id: TransactionId) -> AndromedaResult<()> {
        self.with_machine(id, |machine| machine.request_rollback())
    }

    /// Complete a rollback once the corresponding WAL record is durable.
    pub fn rollback_durable(&self, id: TransactionId, durable_lsn: u64) -> AndromedaResult<()> {
        let mut inner = self.lock();
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
        let mut inner = self.lock();
        let machine = Self::machine_mut(&mut inner.live, id)?;
        machine.apply(crate::state::TransactionEvent::Dispose)?;
        // Only remove once Dispose succeeds (i.e. machine reached Disposed).
        debug_assert_eq!(machine.state, TransactionState::Disposed);
        inner.live.remove(&id);
        Ok(())
    }

    /// Inspect a transaction's current state-machine and mirrored status.
    pub fn snapshot(&self, id: TransactionId) -> Option<TransactionRecord> {
        let inner = self.lock();
        let machine = inner.live.get(&id).copied()?;
        let status = inner.status.status(id)?;
        Some(TransactionRecord {
            state_machine: machine,
            status,
        })
    }

    /// Read-only access to the underlying status table snapshot.
    pub fn status(&self, id: TransactionId) -> Option<TransactionStatus> {
        self.lock().status.status(id)
    }

    /// Borrow the allocator for callers that need to peek (e.g. recovery
    /// instrumentation). The allocator is internally synchronized.
    pub fn allocator(&self) -> &TransactionIdAllocator {
        &self.allocator
    }

    /// Number of live (non-disposed) transactions.
    pub fn live_count(&self) -> usize {
        self.lock().live.len()
    }

    fn with_machine<F>(&self, id: TransactionId, f: F) -> AndromedaResult<()>
    where
        F: FnOnce(&mut TransactionStateMachine) -> AndromedaResult<()>,
    {
        let mut inner = self.lock();
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

    fn lock(&self) -> std::sync::MutexGuard<'_, TransactionManagerInner> {
        // A poisoned mutex here would mean some operation panicked midway,
        // which already represents a fatal invariant violation. Surface it.
        self.inner
            .lock()
            .expect("transaction manager mutex was poisoned")
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
            assert_eq!(mgr.status(id), Some(TransactionStatus::InFlight));
            let snap = mgr.snapshot(id).unwrap();
            assert_eq!(snap.state_machine.state, TransactionState::Active);
            assert_eq!(snap.status, TransactionStatus::InFlight);
        }
    }

    #[test]
    fn commit_path_requires_durable_lsn_before_status_mirrors_committed() {
        let mgr = TransactionManager::new();
        let id = mgr.begin().unwrap();
        mgr.request_commit(id).unwrap();

        // Status is still InFlight until durable evidence arrives.
        assert_eq!(mgr.status(id), Some(TransactionStatus::InFlight));

        // A zero LSN is rejected by the underlying state machine.
        let err = mgr.commit_durable(id, 0).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
        assert_eq!(mgr.status(id), Some(TransactionStatus::InFlight));

        mgr.commit_durable(id, 42).unwrap();
        assert_eq!(mgr.status(id), Some(TransactionStatus::Committed));

        let snap = mgr.snapshot(id).unwrap();
        assert!(snap.state_machine.is_visible_committed());
        assert_eq!(snap.state_machine.durable_commit_lsn, Some(42));
    }

    #[test]
    fn rollback_path_requires_durable_lsn_before_status_mirrors_rolled_back() {
        let mgr = TransactionManager::new();
        let id = mgr.begin().unwrap();
        mgr.request_rollback(id).unwrap();
        assert_eq!(mgr.status(id), Some(TransactionStatus::InFlight));
        mgr.rollback_durable(id, 7).unwrap();
        assert_eq!(mgr.status(id), Some(TransactionStatus::RolledBack));
    }

    #[test]
    fn poison_then_rollback_durable_marks_status_rolled_back() {
        let mgr = TransactionManager::new();
        let id = mgr.begin().unwrap();
        mgr.poison(id).unwrap();
        assert_eq!(mgr.status(id), Some(TransactionStatus::InFlight));
        mgr.request_rollback(id).unwrap();
        mgr.rollback_durable(id, 9).unwrap();
        assert_eq!(mgr.status(id), Some(TransactionStatus::RolledBack));
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
    fn dispose_removes_committed_transaction_but_keeps_status_history() {
        let mgr = TransactionManager::new();
        let id = mgr.begin().unwrap();
        mgr.request_commit(id).unwrap();
        mgr.commit_durable(id, 5).unwrap();
        assert_eq!(mgr.live_count(), 1);

        mgr.dispose(id).unwrap();
        assert_eq!(mgr.live_count(), 0);
        // Status table preserves the historical outcome for visibility.
        assert_eq!(mgr.status(id), Some(TransactionStatus::Committed));
        assert!(mgr.snapshot(id).is_none());
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
        assert_eq!(mgr.status(id), Some(TransactionStatus::InFlight));
    }

    #[test]
    fn status_does_not_mirror_terminal_state_when_state_machine_rejects() {
        // If durable_commit_lsn is zero, the state machine refuses; status
        // table must remain InFlight (no half-mirrored terminal state).
        let mgr = TransactionManager::new();
        let id = mgr.begin().unwrap();
        mgr.request_commit(id).unwrap();
        assert!(mgr.commit_durable(id, 0).is_err());
        assert_eq!(mgr.status(id), Some(TransactionStatus::InFlight));
    }
}
