use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use andromeda_locking::{
    LockAcquireEvidence, LockAcquireStatus, LockManager, LockMode, LockReleaseAllEvidence,
    LockReleaseAllSummary, LockReleaseEvidence, LockResource,
};
use andromeda_mvcc::{TransactionStatus, TransactionStatusTable};
use andromeda_observability::{TraceId, TransitionReasonCode};
use andromeda_savepoint::{
    Savepoint, SavepointReleaseEvidence, SavepointRollbackEvidence, SavepointStack,
};
use andromeda_types::TransactionId;

use crate::allocator::TransactionIdAllocator;
use crate::locking_protocol::{TwoPhaseLocksValidator, TwoPhaseOperation};
use crate::state::{TransactionState, TransactionStateMachine};
use crate::trace::TransactionTransitionCorrelation;
use crate::transition_sink::{NullTransitionSink, TransactionTransitionSink};
use andromeda_transaction_log::Lsn;

use super::lock_coordinator::TransactionLockCoordinator;
use super::record::TransactionRecord;

/// Owner of the live transaction state machines, the status table, and the
/// id allocator.
///
/// Trace emission is observability-only and never authoritative for commit
/// visibility or durable WAL gating; those invariants remain owned by
/// [`TransactionStateMachine`].
pub struct TransactionManager {
    allocator: TransactionIdAllocator,
    inner: Mutex<TransactionManagerInner>,
    /// Pluggable sink that receives a [`andromeda_observability::TransactionTransitionTrace`]
    /// after every successful state-machine transition.
    ///
    /// Defaults to [`NullTransitionSink`] (no-op). Swap with
    /// [`with_transition_sink`][Self::with_transition_sink].
    transition_sink: Arc<dyn TransactionTransitionSink>,
}

impl std::fmt::Debug for TransactionManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransactionManager")
            .field("allocator", &self.allocator)
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
struct TransactionManagerInner {
    live: HashMap<TransactionId, TransactionStateMachine>,
    savepoints: HashMap<TransactionId, SavepointStack>,
    status: TransactionStatusTable,
    /// Per-transaction correlation envelope and trace root id, stored at
    /// [`begin`][TransactionManager::begin] and removed at
    /// [`dispose`][TransactionManager::dispose].
    correlations: HashMap<TransactionId, (TransactionTransitionCorrelation, TraceId)>,
}

impl TransactionManager {
    /// Construct a manager with a fresh allocator (next id = 1).
    pub fn new() -> Self {
        Self {
            allocator: TransactionIdAllocator::new(),
            inner: Mutex::new(TransactionManagerInner::default()),
            transition_sink: Arc::new(NullTransitionSink),
        }
    }

    /// Construct a manager whose allocator floor is set so that the next
    /// allocated id is strictly greater than `floor`. Intended for the
    /// recovery driver.
    pub fn with_recovered_floor(floor: u64) -> Self {
        Self {
            allocator: TransactionIdAllocator::with_floor(floor),
            inner: Mutex::new(TransactionManagerInner::default()),
            transition_sink: Arc::new(NullTransitionSink),
        }
    }

    /// Attach a transition-trace sink to this manager.
    ///
    /// Returns `self` for use in a builder chain. Replaces any previously
    /// configured sink.
    ///
    /// ```rust,ignore
    /// let mgr = TransactionManager::new()
    ///     .with_transition_sink(Arc::new(my_sink));
    /// ```
    pub fn with_transition_sink(mut self, sink: Arc<dyn TransactionTransitionSink>) -> Self {
        self.transition_sink = sink;
        self
    }

    /// Raise the allocator floor during recovery without losing live state.
    pub fn seed_allocator(&self, floor: u64) -> AndromedaResult<()> {
        self.allocator.seed(floor)
    }

    /// Begin a fresh transaction using an empty correlation envelope and a
    /// derived trace id.
    ///
    /// Returns the freshly allocated transaction id. The `Created → Active`
    /// transition trace is emitted to the configured sink.
    ///
    /// See also [`begin_with_correlation`][Self::begin_with_correlation] for
    /// supplying an explicit invocation/request/session context.
    pub fn begin(&self) -> AndromedaResult<TransactionId> {
        let id = self.allocator.allocate()?;
        // Default trace_id derived from the transaction id — unique and
        // deterministic without requiring a separate counter.
        let trace_id = TraceId::new(id.get() as u128);
        self.begin_inner(id, TransactionTransitionCorrelation::empty(), trace_id)
    }

    /// Begin a fresh transaction, associating it with an explicit correlation
    /// envelope and trace root id.
    ///
    /// Emits a `Created → Active` transition trace to the configured sink,
    /// carrying the provided `correlation` and `trace_id` on every subsequent
    /// trace for this transaction's lifetime.
    ///
    /// # Arguments
    ///
    /// * `correlation` – Optional invocation, request, and session ids to
    ///   carry through every trace emitted for this transaction.
    /// * `trace_id` – Non-zero root trace identifier.  Must not be zero; zero
    ///   values will fail [`TransactionTransitionTrace::validate`] if the sink
    ///   runs validation.
    pub fn begin_with_correlation(
        &self,
        correlation: TransactionTransitionCorrelation,
        trace_id: TraceId,
    ) -> AndromedaResult<TransactionId> {
        let id = self.allocator.allocate()?;
        self.begin_inner(id, correlation, trace_id)
    }

    fn begin_inner(
        &self,
        id: TransactionId,
        correlation: TransactionTransitionCorrelation,
        trace_id: TraceId,
    ) -> AndromedaResult<TransactionId> {
        let mut inner = self.lock()?;
        if inner.live.contains_key(&id) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "transaction id allocator returned a live id; allocator corrupted",
            ));
        }
        let mut machine = TransactionStateMachine::new(id);
        let prev_state = machine.state(); // Created
        machine.begin()?;
        // Transition succeeded — snapshot before releasing the lock.
        let machine_snapshot = machine;
        inner.live.insert(id, machine);
        inner.savepoints.insert(id, SavepointStack::new());
        inner.status.record(id, TransactionStatus::InFlight)?;
        inner.correlations.insert(id, (correlation, trace_id));
        drop(inner);

        let trace = machine_snapshot.project_transition(
            trace_id,
            prev_state,
            correlation,
            TransitionReasonCode::NORMAL_PROGRESS,
            "transaction began",
        );
        self.transition_sink.record(trace);
        Ok(id)
    }

    /// Move an active transaction into the `Committing` state. The status
    /// table is *not* updated yet because the commit is not durable.
    pub fn request_commit(&self, id: TransactionId) -> AndromedaResult<()> {
        let mut inner = self.lock()?;
        let machine = Self::machine_mut(&mut inner.live, id)?;
        let prev_state = machine.state();
        machine.request_commit()?;
        let machine_snapshot = *machine;
        let (correlation, trace_id) = Self::get_correlation(&inner.correlations, id);
        if let Some(stack) = inner.savepoints.get_mut(&id) {
            stack.clear();
        }
        drop(inner);

        let trace = machine_snapshot.project_transition(
            trace_id,
            prev_state,
            correlation,
            TransitionReasonCode::NORMAL_PROGRESS,
            "commit requested",
        );
        self.transition_sink.record(trace);
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
    ///
    /// Emits a `Committing → Committed` transition trace with
    /// [`TransitionReasonCode::DURABLE_WAL_FLUSH`] and the resulting
    /// durable LSN populated in the trace.
    pub fn commit_durable_after_wal_record(
        &self,
        id: TransactionId,
        commit_record_lsn: u64,
        durable_lsn: u64,
    ) -> AndromedaResult<()> {
        let mut inner = self.lock()?;
        let machine = Self::machine_mut(&mut inner.live, id)?;
        let prev_state = machine.state();
        machine.publish_visible_commit_with_durable_evidence(commit_record_lsn, durable_lsn)?;
        // State machine guarantees: state == Committed && durable_commit_lsn = Some(_).
        let machine_snapshot = *machine;
        inner.status.record_committed_after_durable_wal(
            id,
            Lsn::new(commit_record_lsn),
            Lsn::new(durable_lsn),
        )?;
        let (correlation, trace_id) = Self::get_correlation(&inner.correlations, id);
        drop(inner);

        let trace = machine_snapshot.project_transition(
            trace_id,
            prev_state,
            correlation,
            TransitionReasonCode::DURABLE_WAL_FLUSH,
            "commit durable after WAL flush",
        );
        self.transition_sink.record(trace);
        Ok(())
    }

    /// Move an active or failed transaction into `RollingBack`. Status stays
    /// `InFlight` until the rollback is durable.
    ///
    /// Reason code is [`TransitionReasonCode::CALLER_ROLLBACK`] for
    /// `Active` and `Poisoned` predecessors, and
    /// [`TransitionReasonCode::EXECUTOR_FAILURE`] when the predecessor was
    /// `Failed`.
    pub fn request_rollback(&self, id: TransactionId) -> AndromedaResult<()> {
        let mut inner = self.lock()?;
        let machine = Self::machine_mut(&mut inner.live, id)?;
        let prev_state = machine.state();
        machine.request_rollback()?;
        let machine_snapshot = *machine;
        let (correlation, trace_id) = Self::get_correlation(&inner.correlations, id);
        if let Some(stack) = inner.savepoints.get_mut(&id) {
            stack.clear();
        }
        drop(inner);

        // Executor-failure predecessor gets a distinct reason code so
        // downstream consumers can distinguish programmer-requested rollbacks
        // from error-driven ones without inspecting the reason string.
        let reason_code = if prev_state == TransactionState::Failed {
            TransitionReasonCode::EXECUTOR_FAILURE
        } else {
            TransitionReasonCode::CALLER_ROLLBACK
        };
        let trace = machine_snapshot.project_transition(
            trace_id,
            prev_state,
            correlation,
            reason_code,
            "rollback requested",
        );
        self.transition_sink.record(trace);
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
    ///
    /// Emits a `RollingBack → RolledBack` transition trace.
    ///
    /// # Reason code choice
    ///
    /// [`TransitionReasonCode::DURABLE_WAL_FLUSH`] is used for the
    /// `RollingBack → RolledBack` trace, consistent with how
    /// `commit_durable` codes the analogous commit terminal boundary.
    /// The prior `request_rollback` trace already carried the originating
    /// reason (CALLER_ROLLBACK or EXECUTOR_FAILURE); the durable completion
    /// trace records *what made the terminal state durable*, not why the
    /// rollback was initiated.
    pub fn rollback_durable_after_wal_record(
        &self,
        id: TransactionId,
        rollback_record_lsn: u64,
        durable_lsn: u64,
    ) -> AndromedaResult<()> {
        let mut inner = self.lock()?;
        let machine = Self::machine_mut(&mut inner.live, id)?;
        let prev_state = machine.state();
        machine.complete_rollback_with_durable_evidence(rollback_record_lsn, durable_lsn)?;
        let machine_snapshot = *machine;
        inner.status.record_rolled_back_after_durable_wal(
            id,
            Lsn::new(rollback_record_lsn),
            Lsn::new(durable_lsn),
        )?;
        let (correlation, trace_id) = Self::get_correlation(&inner.correlations, id);
        drop(inner);

        let trace = machine_snapshot.project_transition(
            trace_id,
            prev_state,
            correlation,
            TransitionReasonCode::DURABLE_WAL_FLUSH,
            "rollback durable after WAL flush",
        );
        self.transition_sink.record(trace);
        Ok(())
    }

    /// Mark a transaction as poisoned. A poisoned transaction must be rolled
    /// back to reach a terminal state; the status table remains `InFlight`
    /// because no durable terminal evidence exists yet.
    pub fn poison(&self, id: TransactionId) -> AndromedaResult<()> {
        let mut inner = self.lock()?;
        let machine = Self::machine_mut(&mut inner.live, id)?;
        let prev_state = machine.state();
        machine.apply(crate::state::TransactionEvent::Poison)?;
        let machine_snapshot = *machine;
        let (correlation, trace_id) = Self::get_correlation(&inner.correlations, id);
        drop(inner);

        let trace = machine_snapshot.project_transition(
            trace_id,
            prev_state,
            correlation,
            TransitionReasonCode::POISON,
            "transaction poisoned",
        );
        self.transition_sink.record(trace);
        Ok(())
    }

    /// Mark a transaction as failed (non-poison failure path).
    pub fn fail(&self, id: TransactionId) -> AndromedaResult<()> {
        let mut inner = self.lock()?;
        let machine = Self::machine_mut(&mut inner.live, id)?;
        let prev_state = machine.state();
        machine.apply(crate::state::TransactionEvent::Fail)?;
        let machine_snapshot = *machine;
        let (correlation, trace_id) = Self::get_correlation(&inner.correlations, id);
        drop(inner);

        let trace = machine_snapshot.project_transition(
            trace_id,
            prev_state,
            correlation,
            TransitionReasonCode::EXECUTOR_FAILURE,
            "transaction failed",
        );
        self.transition_sink.record(trace);
        Ok(())
    }

    /// Dispose of a terminal transaction, removing its live state-machine
    /// entry. The status table retains the historical `Committed` /
    /// `RolledBack` outcome for visibility queries.
    pub fn dispose(&self, id: TransactionId) -> AndromedaResult<()> {
        let mut inner = self.lock()?;
        let machine = Self::machine_mut(&mut inner.live, id)?;
        let prev_state = machine.state();
        machine.apply(crate::state::TransactionEvent::Dispose)?;
        if machine.state() != TransactionState::Disposed {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "transaction dispose did not reach Disposed state",
            ));
        }
        let machine_snapshot = *machine;
        // Remove correlation before dropping the lock; the trace is built
        // outside the lock with the captured values.
        let (correlation, trace_id) = inner.correlations.remove(&id).unwrap_or_else(|| {
            (
                TransactionTransitionCorrelation::empty(),
                TraceId::new(id.get() as u128),
            )
        });
        inner.live.remove(&id);
        inner.savepoints.remove(&id);
        drop(inner);

        let trace = machine_snapshot.project_transition(
            trace_id,
            prev_state,
            correlation,
            TransitionReasonCode::NORMAL_PROGRESS,
            "transaction disposed",
        );
        self.transition_sink.record(trace);
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

    /// Helper for [`TransactionLockCoordinator::acquire`].
    pub fn acquire_lock(
        &self,
        locks: &LockManager,
        tx_id: TransactionId,
        resource: LockResource,
        mode: LockMode,
    ) -> AndromedaResult<LockAcquireStatus> {
        self.lock_coordinator(locks).acquire(tx_id, resource, mode)
    }

    /// Helper for [`TransactionLockCoordinator::acquire_with_evidence`].
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

    /// Helper for [`TransactionLockCoordinator::release`].
    pub fn release_lock(
        &self,
        locks: &LockManager,
        tx_id: TransactionId,
        resource: LockResource,
    ) -> AndromedaResult<bool> {
        self.lock_coordinator(locks).release(tx_id, resource)
    }

    /// Helper for [`TransactionLockCoordinator::release_with_evidence`].
    pub fn release_lock_with_evidence(
        &self,
        locks: &LockManager,
        tx_id: TransactionId,
        resource: LockResource,
    ) -> AndromedaResult<LockReleaseEvidence> {
        self.lock_coordinator(locks)
            .release_with_evidence(tx_id, resource)
    }

    /// Helper for [`TransactionLockCoordinator::release_all`].
    pub fn release_all_locks(
        &self,
        locks: &LockManager,
        tx_id: TransactionId,
    ) -> AndromedaResult<LockReleaseAllSummary> {
        self.lock_coordinator(locks).release_all(tx_id)
    }

    /// Helper for
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

    /// Look up the stored correlation envelope and trace id for `id`.
    ///
    /// Falls back to empty defaults if no entry is found (defensive; should
    /// not occur for valid live transactions).
    fn get_correlation(
        correlations: &HashMap<TransactionId, (TransactionTransitionCorrelation, TraceId)>,
        id: TransactionId,
    ) -> (TransactionTransitionCorrelation, TraceId) {
        correlations.get(&id).copied().unwrap_or_else(|| {
            (
                TransactionTransitionCorrelation::empty(),
                TraceId::new(id.get() as u128),
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
