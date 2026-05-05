//! Deadlock detection vocabulary and deterministic wait-for graph.
//!
//! This module is deliberately storage-agnostic and does not wire any abort or
//! rollback policy into [`crate::TransactionManager`]. It provides stable
//! transaction-level graph derivation and deterministic cycle evidence.
//!
//! State/event checklist for deterministic detection:
//! - States: empty wait-for graph, graph with waiting edges, no cycle reported,
//!   cycle found, detection timed out, detection deferred for future integration
//!   boundaries only.
//! - Events: derive edges from immutable lock-table snapshot, insert wait edge,
//!   remove wait edge, remove transaction, run detector.
//! - Legal transitions: empty -> edge-bearing on validated edge insertion;
//!   edge-bearing -> empty or reduced graph on removal; detector reports
//!   no-cycle for empty or acyclic graphs, cycle-found for the first cycle
//!   reached by deterministic DFS, or timed-out when an injected deadline has
//!   expired.
//! - Illegal transitions: zero transaction identifiers and self-edges are
//!   rejected; timeout policies must be non-zero and bounded.
//! - Audit: lock-table derivation consumes cloned entries only, all edge storage
//!   is deterministic via ordered maps/sets, DFS starts at sorted transaction ids
//!   and follows sorted blockers, victim selection uses explicit transaction
//!   ordering metadata when requested, clock/deadline reads never sleep, and no
//!   abort decision is emitted by the detector.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};

use crate::lock_manager::{LockEntry, LockManager, LockResource};

/// Public deadlock error type.
pub type DeadlockError = AndromedaError;

/// Public deadlock result type.
pub type DeadlockResult<T> = AndromedaResult<T>;

/// Maximum detector timeout accepted by the V0 policy.
pub const MAX_DEADLOCK_TIMEOUT: Duration = Duration::from_secs(60);

/// Default detector timeout accepted by the V0 policy.
pub const DEFAULT_DEADLOCK_TIMEOUT: Duration = Duration::from_millis(500);

const MISSING_TRANSACTION_ORDERING_METADATA_REASON: &str =
    "deadlock transaction ordering metadata missing for cycle participant";
const DEADLOCK_DETECTION_TIMEOUT_REASON: &str = "deadlock detection deadline expired";

/// Deterministic wait-for graph.
///
/// Each edge is `waiting_tx -> blocking_tx`. Ordered storage keeps snapshots and
/// tests reproducible across platforms.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WaitForGraph {
    edges: BTreeMap<TransactionId, BTreeSet<TransactionId>>,
}

impl WaitForGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a wait-for graph from an immutable lock-table snapshot.
    ///
    /// The snapshot is consumed as owned cloned entries, not as references into
    /// the lock manager. For each waiter, the derived graph contains an edge to
    /// every holder whose mode conflicts with the waiter's requested mode.
    /// Waiting lock upgrades are represented by a waiter for a transaction that
    /// may already be in the holder list; that same-transaction holder is skipped
    /// to keep the graph free of self-edges while preserving edges to other
    /// incompatible holders.
    pub fn from_lock_table_snapshot<I>(snapshot: I) -> DeadlockResult<Self>
    where
        I: IntoIterator<Item = (LockResource, LockEntry)>,
    {
        let mut graph = Self::new();

        for (_resource, entry) in snapshot {
            let LockEntry { holders, waiters } = entry;

            for waiter in waiters {
                validate_transaction_id(waiter.tx_id)?;

                for holder in &holders {
                    validate_transaction_id(holder.tx_id)?;

                    if holder.tx_id == waiter.tx_id {
                        continue;
                    }

                    if !holder.mode.is_compatible_with(waiter.mode) {
                        graph.insert_edge(waiter.tx_id, holder.tx_id)?;
                    }
                }
            }
        }

        Ok(graph)
    }

    /// Build a wait-for graph from the lock manager's deterministic cloned
    /// snapshot without exposing mutable lock-table internals.
    pub fn from_lock_manager(lock_manager: &LockManager) -> DeadlockResult<Self> {
        Self::from_lock_table_snapshot(lock_manager.snapshot()?)
    }

    /// Insert a wait-for edge.
    ///
    /// Returns `true` when the graph changed and `false` when the edge already
    /// existed.
    pub fn insert_edge(
        &mut self,
        waiting_tx: TransactionId,
        blocking_tx: TransactionId,
    ) -> DeadlockResult<bool> {
        validate_transaction_id(waiting_tx)?;
        validate_transaction_id(blocking_tx)?;
        validate_not_self_edge(waiting_tx, blocking_tx)?;

        Ok(self
            .edges
            .entry(waiting_tx)
            .or_default()
            .insert(blocking_tx))
    }

    /// Remove one wait-for edge.
    ///
    /// Returns `true` when an edge was removed.
    pub fn remove_edge(
        &mut self,
        waiting_tx: TransactionId,
        blocking_tx: TransactionId,
    ) -> DeadlockResult<bool> {
        validate_transaction_id(waiting_tx)?;
        validate_transaction_id(blocking_tx)?;
        validate_not_self_edge(waiting_tx, blocking_tx)?;

        let Some(blockers) = self.edges.get_mut(&waiting_tx) else {
            return Ok(false);
        };

        let removed = blockers.remove(&blocking_tx);
        if blockers.is_empty() {
            self.edges.remove(&waiting_tx);
        }

        Ok(removed)
    }

    /// Remove all outgoing and incoming edges for one transaction.
    ///
    /// Returns `true` when the graph changed.
    pub fn remove_transaction(&mut self, tx_id: TransactionId) -> DeadlockResult<bool> {
        validate_transaction_id(tx_id)?;

        let removed_outgoing = self.edges.remove(&tx_id).is_some();
        let mut removed_incoming = false;

        self.edges.retain(|_, blockers| {
            if blockers.remove(&tx_id) {
                removed_incoming = true;
            }
            !blockers.is_empty()
        });

        Ok(removed_outgoing || removed_incoming)
    }

    /// Return blockers for a waiting transaction in deterministic order.
    pub fn blockers_for(&self, waiting_tx: TransactionId) -> DeadlockResult<Vec<TransactionId>> {
        validate_transaction_id(waiting_tx)?;
        Ok(self
            .edges
            .get(&waiting_tx)
            .map(|blockers| blockers.iter().copied().collect())
            .unwrap_or_default())
    }

    /// Return all edges as sorted `(waiting_tx, blocking_tx)` pairs.
    pub fn edges(&self) -> Vec<(TransactionId, TransactionId)> {
        self.edges
            .iter()
            .flat_map(|(waiting_tx, blockers)| {
                blockers
                    .iter()
                    .map(move |blocking_tx| (*waiting_tx, *blocking_tx))
            })
            .collect()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.values().map(BTreeSet::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    fn validate(&self) -> DeadlockResult<()> {
        for (waiting_tx, blockers) in &self.edges {
            validate_transaction_id(*waiting_tx)?;
            for blocking_tx in blockers {
                validate_transaction_id(*blocking_tx)?;
                validate_not_self_edge(*waiting_tx, *blocking_tx)?;
            }
        }

        Ok(())
    }
}

/// Storage-agnostic transaction ordering metadata used for victim selection.
///
/// `start_order` is a monotonic transaction-begin order supplied by the
/// transaction layer. Larger values are treated as younger transactions. Equal
/// values are legal so import/replay callers can model coarse ordering; ties are
/// broken by the greatest [`TransactionId`] to keep victim choice deterministic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeadlockTransactionMetadata {
    pub tx_id: TransactionId,
    pub start_order: u64,
}

impl DeadlockTransactionMetadata {
    pub fn new(tx_id: TransactionId, start_order: u64) -> DeadlockResult<Self> {
        validate_transaction_id(tx_id)?;
        validate_start_order(start_order)?;

        Ok(Self { tx_id, start_order })
    }

    fn validate(self) -> DeadlockResult<()> {
        validate_transaction_id(self.tx_id)?;
        validate_start_order(self.start_order)?;
        Ok(())
    }
}

/// Deterministic, storage-agnostic table for transaction ordering metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeadlockTransactionMetadataTable {
    entries: BTreeMap<TransactionId, DeadlockTransactionMetadata>,
}

impl DeadlockTransactionMetadataTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or replace ordering metadata for one transaction.
    ///
    /// Returns the previously registered metadata when an entry is replaced.
    pub fn register(
        &mut self,
        metadata: DeadlockTransactionMetadata,
    ) -> DeadlockResult<Option<DeadlockTransactionMetadata>> {
        metadata.validate()?;
        Ok(self.entries.insert(metadata.tx_id, metadata))
    }

    /// Register or replace ordering metadata by transaction id and start order.
    pub fn register_start_order(
        &mut self,
        tx_id: TransactionId,
        start_order: u64,
    ) -> DeadlockResult<Option<DeadlockTransactionMetadata>> {
        self.register(DeadlockTransactionMetadata::new(tx_id, start_order)?)
    }

    pub fn metadata_for(
        &self,
        tx_id: TransactionId,
    ) -> DeadlockResult<Option<DeadlockTransactionMetadata>> {
        validate_transaction_id(tx_id)?;
        Ok(self.entries.get(&tx_id).copied())
    }

    pub fn contains(&self, tx_id: TransactionId) -> DeadlockResult<bool> {
        validate_transaction_id(tx_id)?;
        Ok(self.entries.contains_key(&tx_id))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn has_metadata_for_all(&self, tx_ids: &[TransactionId]) -> DeadlockResult<bool> {
        for tx_id in tx_ids {
            validate_transaction_id(*tx_id)?;
            if !self.entries.contains_key(tx_id) {
                return Ok(false);
            }
        }

        Ok(true)
    }
}

/// Deterministic policy for selecting a victim after a cycle is found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlockVictimPolicy {
    /// Choose the greatest transaction start/order value.
    ///
    /// Requires [`DeadlockTransactionMetadataTable`]. Ties are broken by the
    /// greatest transaction identifier.
    YoungestTransactionStartOrder,
    /// Choose the greatest transaction identifier without start/order metadata.
    YoungestTransactionId,
    /// Choose the smallest transaction identifier.
    OldestTransactionId,
}

/// Bounded deadlock policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeadlockPolicy {
    pub detection_timeout: Duration,
    pub victim_policy: DeadlockVictimPolicy,
}

impl DeadlockPolicy {
    pub fn new(
        detection_timeout: Duration,
        victim_policy: DeadlockVictimPolicy,
    ) -> DeadlockResult<Self> {
        let policy = Self {
            detection_timeout,
            victim_policy,
        };
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(self) -> DeadlockResult<()> {
        validate_deadlock_timeout(self.detection_timeout)
    }
}

impl Default for DeadlockPolicy {
    fn default() -> Self {
        Self {
            detection_timeout: DEFAULT_DEADLOCK_TIMEOUT,
            victim_policy: DeadlockVictimPolicy::YoungestTransactionStartOrder,
        }
    }
}

/// Storage/runtime-agnostic deadlock clock instant.
///
/// The value is a monotonic duration on an arbitrary clock timeline. It is not a
/// wall-clock timestamp and carries no durability or recovery meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeadlockClockInstant {
    elapsed_since_clock_start: Duration,
}

impl DeadlockClockInstant {
    pub const fn from_elapsed_since_clock_start(elapsed_since_clock_start: Duration) -> Self {
        Self {
            elapsed_since_clock_start,
        }
    }

    pub const fn elapsed_since_clock_start(self) -> Duration {
        self.elapsed_since_clock_start
    }

    pub fn elapsed_since(self, earlier: Self) -> Duration {
        self.elapsed_since_clock_start
            .saturating_sub(earlier.elapsed_since_clock_start)
    }
}

/// Injectable clock used by deadline-aware deadlock detection.
///
/// Implementations must be side-effect-free from the detector's point of view:
/// reading the clock may not mutate the wait-for graph, lock table, transaction
/// state, or durability state.
pub trait DeadlockClock {
    fn now(&self) -> DeadlockClockInstant;
}

/// Runtime monotonic clock adapter for deadlock detection.
///
/// This adapter reads [`Instant::now`] and never sleeps. Tests should prefer
/// [`ManualDeadlockClock`] for deterministic control.
#[derive(Debug, Clone)]
pub struct SystemDeadlockClock {
    started_at: Instant,
}

impl SystemDeadlockClock {
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }
}

impl Default for SystemDeadlockClock {
    fn default() -> Self {
        Self::new()
    }
}

impl DeadlockClock for SystemDeadlockClock {
    fn now(&self) -> DeadlockClockInstant {
        DeadlockClockInstant::from_elapsed_since_clock_start(self.started_at.elapsed())
    }
}

/// Deterministic manual clock for unit tests and runtime simulations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManualDeadlockClock {
    now: DeadlockClockInstant,
}

impl ManualDeadlockClock {
    pub const fn new() -> Self {
        Self {
            now: DeadlockClockInstant::from_elapsed_since_clock_start(Duration::ZERO),
        }
    }

    pub const fn at(elapsed_since_clock_start: Duration) -> Self {
        Self {
            now: DeadlockClockInstant::from_elapsed_since_clock_start(elapsed_since_clock_start),
        }
    }

    pub fn advance(&mut self, elapsed: Duration) -> DeadlockResult<()> {
        let Some(next) = self.now.elapsed_since_clock_start.checked_add(elapsed) else {
            return Err(deadlock_error("deadlock manual clock advance overflowed"));
        };
        self.now = DeadlockClockInstant::from_elapsed_since_clock_start(next);
        Ok(())
    }

    pub fn set(&mut self, elapsed_since_clock_start: Duration) {
        self.now = DeadlockClockInstant::from_elapsed_since_clock_start(elapsed_since_clock_start);
    }
}

impl Default for ManualDeadlockClock {
    fn default() -> Self {
        Self::new()
    }
}

impl DeadlockClock for ManualDeadlockClock {
    fn now(&self) -> DeadlockClockInstant {
        self.now
    }
}

/// Deadline for one side-effect-free deadlock detection attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeadlockDetectionDeadline {
    started_at: DeadlockClockInstant,
    timeout: Duration,
}

impl DeadlockDetectionDeadline {
    pub fn new(started_at: DeadlockClockInstant, timeout: Duration) -> DeadlockResult<Self> {
        validate_deadlock_timeout(timeout)?;
        Ok(Self {
            started_at,
            timeout,
        })
    }

    pub fn from_policy<C: DeadlockClock>(
        policy: DeadlockPolicy,
        clock: &C,
    ) -> DeadlockResult<Self> {
        policy.validate()?;
        Self::new(clock.now(), policy.detection_timeout)
    }

    pub const fn started_at(self) -> DeadlockClockInstant {
        self.started_at
    }

    pub const fn timeout(self) -> Duration {
        self.timeout
    }

    pub fn elapsed_at(self, now: DeadlockClockInstant) -> Duration {
        now.elapsed_since(self.started_at)
    }

    pub fn is_expired_at(self, now: DeadlockClockInstant) -> bool {
        self.elapsed_at(now) >= self.timeout
    }

    pub fn is_expired<C: DeadlockClock>(self, clock: &C) -> bool {
        self.is_expired_at(clock.now())
    }
}

/// Deterministic deadlock victim description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlockVictim {
    pub tx_id: TransactionId,
    pub victim_policy: DeadlockVictimPolicy,
    pub cycle_participants: Vec<TransactionId>,
}

impl DeadlockVictim {
    pub fn new(
        tx_id: TransactionId,
        victim_policy: DeadlockVictimPolicy,
        cycle_participants: Vec<TransactionId>,
    ) -> DeadlockResult<Self> {
        validate_transaction_id(tx_id)?;
        validate_cycle_participants(&cycle_participants)?;

        if !cycle_participants.contains(&tx_id) {
            return Err(deadlock_error(
                "deadlock victim must be a member of cycle participants",
            ));
        }

        Ok(Self {
            tx_id,
            victim_policy,
            cycle_participants: sorted_unique(cycle_participants),
        })
    }

    pub fn from_cycle(
        cycle_participants: Vec<TransactionId>,
        victim_policy: DeadlockVictimPolicy,
    ) -> DeadlockResult<Self> {
        validate_cycle_participants(&cycle_participants)?;

        let sorted_participants = sorted_unique(cycle_participants);
        let tx_id = match victim_policy {
            DeadlockVictimPolicy::YoungestTransactionStartOrder => {
                return Err(deadlock_error(
                    "deadlock youngest transaction policy requires ordering metadata",
                ));
            }
            DeadlockVictimPolicy::YoungestTransactionId => sorted_participants
                .last()
                .copied()
                .ok_or_else(|| deadlock_error("deadlock cycle must not be empty"))?,
            DeadlockVictimPolicy::OldestTransactionId => sorted_participants
                .first()
                .copied()
                .ok_or_else(|| deadlock_error("deadlock cycle must not be empty"))?,
        };

        Ok(Self {
            tx_id,
            victim_policy,
            cycle_participants: sorted_participants,
        })
    }

    pub fn from_cycle_with_transaction_metadata(
        cycle_participants: Vec<TransactionId>,
        victim_policy: DeadlockVictimPolicy,
        transaction_metadata: &DeadlockTransactionMetadataTable,
    ) -> DeadlockResult<Self> {
        validate_cycle_participants(&cycle_participants)?;

        let sorted_participants = sorted_unique(cycle_participants);
        let tx_id = match victim_policy {
            DeadlockVictimPolicy::YoungestTransactionStartOrder => {
                select_youngest_by_start_order(&sorted_participants, transaction_metadata)?
            }
            DeadlockVictimPolicy::YoungestTransactionId => sorted_participants
                .last()
                .copied()
                .ok_or_else(|| deadlock_error("deadlock cycle must not be empty"))?,
            DeadlockVictimPolicy::OldestTransactionId => sorted_participants
                .first()
                .copied()
                .ok_or_else(|| deadlock_error("deadlock cycle must not be empty"))?,
        };

        Ok(Self {
            tx_id,
            victim_policy,
            cycle_participants: sorted_participants,
        })
    }
}

/// Detector outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeadlockDetectionStatus {
    NoCycle,
    CycleFound {
        victim: DeadlockVictim,
        cycle_participants: Vec<TransactionId>,
    },
    TimedOut {
        reason: &'static str,
        elapsed: Duration,
        timeout: Duration,
    },
    Deferred {
        reason: &'static str,
    },
}

/// Side-effect-free evidence captured for a coordinator-facing deadlock decision.
///
/// The evidence is derived from an owned lock-table snapshot. It contains no
/// references into the lock manager and carries no commit, rollback, abort, or
/// durability semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlockDecisionEvidence {
    /// Number of resource entries in the lock-table snapshot that was observed.
    pub lock_snapshot_resource_count: usize,
    /// Deterministic wait-for edges derived from that snapshot.
    pub wait_for_edges: Vec<(TransactionId, TransactionId)>,
}

impl DeadlockDecisionEvidence {
    pub fn new(lock_snapshot_resource_count: usize, graph: &WaitForGraph) -> Self {
        Self {
            lock_snapshot_resource_count,
            wait_for_edges: graph.edges(),
        }
    }

    pub fn wait_for_edge_count(&self) -> usize {
        self.wait_for_edges.len()
    }
}

/// Boundary-safe decision returned to a transaction coordinator.
///
/// `VictimSuggested` is evidence only. The detector never mutates the lock
/// manager and never invokes transaction abort, rollback, or durability code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeadlockDecision {
    NoCycle {
        evidence: DeadlockDecisionEvidence,
    },
    VictimSuggested {
        victim: DeadlockVictim,
        cycle_participants: Vec<TransactionId>,
        evidence: DeadlockDecisionEvidence,
    },
    Deferred {
        reason: &'static str,
        evidence: DeadlockDecisionEvidence,
    },
    TimedOut {
        reason: &'static str,
        elapsed: Duration,
        timeout: Duration,
        evidence: DeadlockDecisionEvidence,
    },
}

/// Structured deadlock trace outcome for transaction-local audit projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlockDecisionTraceOutcome {
    NoCycle,
    VictimSuggested,
    Deferred,
    TimedOut,
}

/// Correlation-friendly deadlock decision trace.
///
/// This projection contains detector facts only. It does not mutate the lock
/// table or transaction manager and intentionally carries no durable LSN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlockDecisionTrace {
    /// The suggested victim transaction for `VictimSuggested`; otherwise `None`.
    pub tx_id: Option<TransactionId>,
    pub wait_for_edges: Vec<(TransactionId, TransactionId)>,
    pub cycle_participants: Vec<TransactionId>,
    pub outcome: DeadlockDecisionTraceOutcome,
    pub reason: &'static str,
    pub lock_snapshot_resource_count: usize,
}

impl DeadlockDecision {
    /// Project this decision into local audit evidence.
    ///
    /// The projection reports a victim suggestion, timeout, deferral, or no-cycle
    /// observation only; it is not an abort/rollback instruction.
    pub fn to_trace(&self) -> DeadlockDecisionTrace {
        match self {
            Self::NoCycle { evidence } => DeadlockDecisionTrace {
                tx_id: None,
                wait_for_edges: evidence.wait_for_edges.clone(),
                cycle_participants: Vec::new(),
                outcome: DeadlockDecisionTraceOutcome::NoCycle,
                reason: "deadlock_detection_found_no_cycle",
                lock_snapshot_resource_count: evidence.lock_snapshot_resource_count,
            },
            Self::VictimSuggested {
                victim,
                cycle_participants,
                evidence,
            } => DeadlockDecisionTrace {
                tx_id: Some(victim.tx_id),
                wait_for_edges: evidence.wait_for_edges.clone(),
                cycle_participants: cycle_participants.clone(),
                outcome: DeadlockDecisionTraceOutcome::VictimSuggested,
                reason: "deadlock_detection_suggested_victim",
                lock_snapshot_resource_count: evidence.lock_snapshot_resource_count,
            },
            Self::Deferred { reason, evidence } => DeadlockDecisionTrace {
                tx_id: None,
                wait_for_edges: evidence.wait_for_edges.clone(),
                cycle_participants: Vec::new(),
                outcome: DeadlockDecisionTraceOutcome::Deferred,
                reason,
                lock_snapshot_resource_count: evidence.lock_snapshot_resource_count,
            },
            Self::TimedOut {
                reason, evidence, ..
            } => DeadlockDecisionTrace {
                tx_id: None,
                wait_for_edges: evidence.wait_for_edges.clone(),
                cycle_participants: Vec::new(),
                outcome: DeadlockDecisionTraceOutcome::TimedOut,
                reason,
                lock_snapshot_resource_count: evidence.lock_snapshot_resource_count,
            },
        }
    }
}

/// Optional coordinator inputs for one side-effect-free decision attempt.
#[derive(Debug, Clone, Copy)]
pub struct DeadlockDecisionOptions<'a, C>
where
    C: DeadlockClock,
{
    pub transaction_metadata: Option<&'a DeadlockTransactionMetadataTable>,
    pub deadline: Option<DeadlockDetectionDeadline>,
    pub clock: &'a C,
}

impl<'a, C> DeadlockDecisionOptions<'a, C>
where
    C: DeadlockClock,
{
    pub fn new(clock: &'a C) -> Self {
        Self {
            transaction_metadata: None,
            deadline: None,
            clock,
        }
    }

    pub fn with_transaction_metadata(
        mut self,
        transaction_metadata: &'a DeadlockTransactionMetadataTable,
    ) -> Self {
        self.transaction_metadata = Some(transaction_metadata);
        self
    }

    pub fn with_deadline(mut self, deadline: DeadlockDetectionDeadline) -> Self {
        self.deadline = Some(deadline);
        self
    }
}

/// Build a coordinator-facing deadlock decision from an owned lock-table
/// snapshot using the policy timeout and a deterministic zero-elapsed clock.
///
/// This function is side-effect-free with respect to the lock manager because it
/// accepts owned snapshot entries only.
pub fn decide_deadlock_from_lock_table_snapshot<I>(
    snapshot: I,
    policy: DeadlockPolicy,
    transaction_metadata: Option<&DeadlockTransactionMetadataTable>,
) -> DeadlockResult<DeadlockDecision>
where
    I: IntoIterator<Item = (LockResource, LockEntry)>,
{
    let clock = ManualDeadlockClock::new();
    let deadline = DeadlockDetectionDeadline::from_policy(policy, &clock)?;
    decide_deadlock_from_lock_table_snapshot_with_deadline(
        snapshot,
        policy,
        transaction_metadata,
        deadline,
        &clock,
    )
}

/// Build a coordinator-facing deadlock decision from an owned lock-table
/// snapshot plus optional metadata/deadline inputs.
pub fn decide_deadlock_from_lock_table_snapshot_with_options<I, C>(
    snapshot: I,
    policy: DeadlockPolicy,
    options: DeadlockDecisionOptions<'_, C>,
) -> DeadlockResult<DeadlockDecision>
where
    I: IntoIterator<Item = (LockResource, LockEntry)>,
    C: DeadlockClock,
{
    let deadline = match options.deadline {
        Some(deadline) => deadline,
        None => DeadlockDetectionDeadline::from_policy(policy, options.clock)?,
    };

    decide_deadlock_from_lock_table_snapshot_with_deadline(
        snapshot,
        policy,
        options.transaction_metadata,
        deadline,
        options.clock,
    )
}

/// Build a coordinator-facing deadlock decision from an owned lock-table
/// snapshot and an injected deadline/clock pair.
///
/// The decision API observes lock snapshot state, derives graph evidence, and
/// returns a typed decision only. It does not mutate lock queues, remove waiters,
/// release locks, abort transactions, roll back transactions, or claim durable
/// outcome.
pub fn decide_deadlock_from_lock_table_snapshot_with_deadline<I, C>(
    snapshot: I,
    policy: DeadlockPolicy,
    transaction_metadata: Option<&DeadlockTransactionMetadataTable>,
    deadline: DeadlockDetectionDeadline,
    clock: &C,
) -> DeadlockResult<DeadlockDecision>
where
    I: IntoIterator<Item = (LockResource, LockEntry)>,
    C: DeadlockClock,
{
    let snapshot: Vec<(LockResource, LockEntry)> = snapshot.into_iter().collect();
    let lock_snapshot_resource_count = snapshot.len();
    let graph = WaitForGraph::from_lock_table_snapshot(snapshot)?;
    let evidence = DeadlockDecisionEvidence::new(lock_snapshot_resource_count, &graph);
    let detector = DeadlockDetector::new(policy)?;
    let empty_metadata;
    let transaction_metadata = match transaction_metadata {
        Some(transaction_metadata) => transaction_metadata,
        None => {
            empty_metadata = DeadlockTransactionMetadataTable::new();
            &empty_metadata
        }
    };

    let status = detector.detect_with_transaction_metadata_and_deadline(
        &graph,
        transaction_metadata,
        deadline,
        clock,
    )?;

    Ok(decision_from_detection_status(status, evidence))
}

/// Observe a lock manager through its cloned deterministic snapshot and return a
/// coordinator-facing deadlock decision.
///
/// This is a convenience boundary around [`LockManager::snapshot`]; it preserves
/// lock-manager state and returns evidence only.
pub fn decide_deadlock_from_lock_manager(
    lock_manager: &LockManager,
    policy: DeadlockPolicy,
    transaction_metadata: Option<&DeadlockTransactionMetadataTable>,
) -> DeadlockResult<DeadlockDecision> {
    decide_deadlock_from_lock_table_snapshot(lock_manager.snapshot()?, policy, transaction_metadata)
}

/// Observe a lock manager snapshot and return a decision using optional
/// metadata/deadline inputs.
pub fn decide_deadlock_from_lock_manager_with_options<C>(
    lock_manager: &LockManager,
    policy: DeadlockPolicy,
    options: DeadlockDecisionOptions<'_, C>,
) -> DeadlockResult<DeadlockDecision>
where
    C: DeadlockClock,
{
    decide_deadlock_from_lock_table_snapshot_with_options(lock_manager.snapshot()?, policy, options)
}

/// Observe a lock manager through its cloned deterministic snapshot and return a
/// coordinator-facing decision using an injected deadline/clock pair.
pub fn decide_deadlock_from_lock_manager_with_deadline<C>(
    lock_manager: &LockManager,
    policy: DeadlockPolicy,
    transaction_metadata: Option<&DeadlockTransactionMetadataTable>,
    deadline: DeadlockDetectionDeadline,
    clock: &C,
) -> DeadlockResult<DeadlockDecision>
where
    C: DeadlockClock,
{
    decide_deadlock_from_lock_table_snapshot_with_deadline(
        lock_manager.snapshot()?,
        policy,
        transaction_metadata,
        deadline,
        clock,
    )
}

/// Deadlock detector.
///
/// This type validates policy and graph inputs, then runs deterministic DFS over
/// sorted transaction identifiers and sorted wait-for blockers. It reports a
/// victim description only; it does not invoke transaction abort or rollback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeadlockDetector {
    policy: DeadlockPolicy,
}

impl DeadlockDetector {
    pub fn new(policy: DeadlockPolicy) -> DeadlockResult<Self> {
        policy.validate()?;
        Ok(Self { policy })
    }

    pub fn policy(self) -> DeadlockPolicy {
        self.policy
    }

    pub fn detect(&self, graph: &WaitForGraph) -> DeadlockResult<DeadlockDetectionStatus> {
        self.detect_with_transaction_metadata(graph, &DeadlockTransactionMetadataTable::new())
    }

    pub fn detect_with_transaction_metadata(
        &self,
        graph: &WaitForGraph,
        transaction_metadata: &DeadlockTransactionMetadataTable,
    ) -> DeadlockResult<DeadlockDetectionStatus> {
        let clock = ManualDeadlockClock::new();
        let deadline = DeadlockDetectionDeadline::from_policy(self.policy, &clock)?;
        self.detect_with_transaction_metadata_and_deadline(
            graph,
            transaction_metadata,
            deadline,
            &clock,
        )
    }

    pub fn detect_with_deadline<C: DeadlockClock>(
        &self,
        graph: &WaitForGraph,
        deadline: DeadlockDetectionDeadline,
        clock: &C,
    ) -> DeadlockResult<DeadlockDetectionStatus> {
        self.detect_with_transaction_metadata_and_deadline(
            graph,
            &DeadlockTransactionMetadataTable::new(),
            deadline,
            clock,
        )
    }

    pub fn detect_with_transaction_metadata_and_deadline<C: DeadlockClock>(
        &self,
        graph: &WaitForGraph,
        transaction_metadata: &DeadlockTransactionMetadataTable,
        deadline: DeadlockDetectionDeadline,
        clock: &C,
    ) -> DeadlockResult<DeadlockDetectionStatus> {
        self.policy.validate()?;
        validate_deadlock_timeout(deadline.timeout)?;
        if deadline.timeout > self.policy.detection_timeout {
            return Err(deadlock_error(
                "deadlock detection deadline exceeds policy timeout",
            ));
        }

        if let Some(status) = timeout_status_if_expired(deadline, clock) {
            return Ok(status);
        }

        graph.validate()?;

        if graph.is_empty() {
            return Ok(DeadlockDetectionStatus::NoCycle);
        }

        let cycle_participants = match find_first_cycle_until(graph, deadline, clock)? {
            CycleSearchOutcome::CycleFound(cycle_participants) => cycle_participants,
            CycleSearchOutcome::NoCycle => return Ok(DeadlockDetectionStatus::NoCycle),
            CycleSearchOutcome::TimedOut => return Ok(timeout_status(deadline, clock.now())),
        };

        if self.policy.victim_policy == DeadlockVictimPolicy::YoungestTransactionStartOrder
            && !transaction_metadata.has_metadata_for_all(&cycle_participants)?
        {
            return Ok(DeadlockDetectionStatus::Deferred {
                reason: MISSING_TRANSACTION_ORDERING_METADATA_REASON,
            });
        }

        let victim = DeadlockVictim::from_cycle_with_transaction_metadata(
            cycle_participants,
            self.policy.victim_policy,
            transaction_metadata,
        )?;
        let cycle_participants = victim.cycle_participants.clone();

        Ok(DeadlockDetectionStatus::CycleFound {
            victim,
            cycle_participants,
        })
    }
}

impl Default for DeadlockDetector {
    fn default() -> Self {
        Self {
            policy: DeadlockPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DfsVisitState {
    Visiting,
    Visited,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CycleSearchOutcome {
    NoCycle,
    CycleFound(Vec<TransactionId>),
    TimedOut,
}

fn find_first_cycle_until<C: DeadlockClock>(
    graph: &WaitForGraph,
    deadline: DeadlockDetectionDeadline,
    clock: &C,
) -> DeadlockResult<CycleSearchOutcome> {
    let mut states = BTreeMap::<TransactionId, DfsVisitState>::new();
    let mut stack = Vec::<TransactionId>::new();

    for tx_id in sorted_transaction_ids(graph) {
        if deadline.is_expired(clock) {
            return Ok(CycleSearchOutcome::TimedOut);
        }

        if states.contains_key(&tx_id) {
            continue;
        }

        match dfs_cycle_from_until(tx_id, graph, &mut states, &mut stack, deadline, clock)? {
            CycleSearchOutcome::CycleFound(cycle_participants) => {
                return Ok(CycleSearchOutcome::CycleFound(cycle_participants));
            }
            CycleSearchOutcome::TimedOut => return Ok(CycleSearchOutcome::TimedOut),
            CycleSearchOutcome::NoCycle => {}
        }
    }

    Ok(CycleSearchOutcome::NoCycle)
}

fn sorted_transaction_ids(graph: &WaitForGraph) -> Vec<TransactionId> {
    graph
        .edges
        .iter()
        .flat_map(|(waiting_tx, blockers)| {
            std::iter::once(*waiting_tx).chain(blockers.iter().copied())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn dfs_cycle_from_until<C: DeadlockClock>(
    tx_id: TransactionId,
    graph: &WaitForGraph,
    states: &mut BTreeMap<TransactionId, DfsVisitState>,
    stack: &mut Vec<TransactionId>,
    deadline: DeadlockDetectionDeadline,
    clock: &C,
) -> DeadlockResult<CycleSearchOutcome> {
    if deadline.is_expired(clock) {
        return Ok(CycleSearchOutcome::TimedOut);
    }

    states.insert(tx_id, DfsVisitState::Visiting);
    stack.push(tx_id);

    if let Some(blockers) = graph.edges.get(&tx_id) {
        for blocking_tx in blockers {
            if deadline.is_expired(clock) {
                return Ok(CycleSearchOutcome::TimedOut);
            }

            match states.get(blocking_tx).copied() {
                Some(DfsVisitState::Visiting) => {
                    if let Some(cycle_start) = stack
                        .iter()
                        .position(|stacked_tx| stacked_tx == blocking_tx)
                    {
                        return Ok(CycleSearchOutcome::CycleFound(
                            stack[cycle_start..].to_vec(),
                        ));
                    }
                }
                Some(DfsVisitState::Visited) => {}
                None => {
                    match dfs_cycle_from_until(*blocking_tx, graph, states, stack, deadline, clock)?
                    {
                        CycleSearchOutcome::CycleFound(cycle_participants) => {
                            return Ok(CycleSearchOutcome::CycleFound(cycle_participants));
                        }
                        CycleSearchOutcome::TimedOut => return Ok(CycleSearchOutcome::TimedOut),
                        CycleSearchOutcome::NoCycle => {}
                    }
                }
            }
        }
    }

    stack.pop();
    states.insert(tx_id, DfsVisitState::Visited);
    Ok(CycleSearchOutcome::NoCycle)
}

fn validate_deadlock_timeout(detection_timeout: Duration) -> DeadlockResult<()> {
    if detection_timeout.is_zero() {
        return Err(deadlock_error(
            "deadlock detection timeout must not be zero",
        ));
    }

    if detection_timeout > MAX_DEADLOCK_TIMEOUT {
        return Err(deadlock_error(
            "deadlock detection timeout exceeds V0 bound",
        ));
    }

    Ok(())
}

fn validate_transaction_id(tx_id: TransactionId) -> DeadlockResult<()> {
    if tx_id.get() == 0 {
        return Err(deadlock_error("deadlock transaction id must not be zero"));
    }

    Ok(())
}

fn validate_start_order(start_order: u64) -> DeadlockResult<()> {
    if start_order == 0 {
        return Err(deadlock_error(
            "deadlock transaction start order must not be zero",
        ));
    }

    Ok(())
}

fn validate_not_self_edge(
    waiting_tx: TransactionId,
    blocking_tx: TransactionId,
) -> DeadlockResult<()> {
    if waiting_tx == blocking_tx {
        return Err(deadlock_error("deadlock wait-for self edge is invalid"));
    }

    Ok(())
}

fn validate_cycle_participants(cycle_participants: &[TransactionId]) -> DeadlockResult<()> {
    if cycle_participants.len() < 2 {
        return Err(deadlock_error(
            "deadlock cycle must contain at least two transactions",
        ));
    }

    for tx_id in cycle_participants {
        validate_transaction_id(*tx_id)?;
    }

    Ok(())
}

fn sorted_unique(mut tx_ids: Vec<TransactionId>) -> Vec<TransactionId> {
    tx_ids.sort();
    tx_ids.dedup();
    tx_ids
}

fn select_youngest_by_start_order(
    sorted_participants: &[TransactionId],
    transaction_metadata: &DeadlockTransactionMetadataTable,
) -> DeadlockResult<TransactionId> {
    let mut selected: Option<(u64, TransactionId)> = None;

    for tx_id in sorted_participants {
        let Some(metadata) = transaction_metadata.metadata_for(*tx_id)? else {
            return Err(deadlock_error(MISSING_TRANSACTION_ORDERING_METADATA_REASON));
        };
        let candidate = (metadata.start_order, *tx_id);
        if selected.map(|current| candidate > current).unwrap_or(true) {
            selected = Some(candidate);
        }
    }

    selected
        .map(|(_start_order, tx_id)| tx_id)
        .ok_or_else(|| deadlock_error("deadlock cycle must not be empty"))
}

fn deadlock_error(message: &'static str) -> DeadlockError {
    AndromedaError::new(AndromedaErrorKind::Transaction, message)
}

fn timeout_status_if_expired<C: DeadlockClock>(
    deadline: DeadlockDetectionDeadline,
    clock: &C,
) -> Option<DeadlockDetectionStatus> {
    let now = clock.now();
    deadline
        .is_expired_at(now)
        .then(|| timeout_status(deadline, now))
}

fn timeout_status(
    deadline: DeadlockDetectionDeadline,
    now: DeadlockClockInstant,
) -> DeadlockDetectionStatus {
    DeadlockDetectionStatus::TimedOut {
        reason: DEADLOCK_DETECTION_TIMEOUT_REASON,
        elapsed: deadline.elapsed_at(now),
        timeout: deadline.timeout,
    }
}

fn decision_from_detection_status(
    status: DeadlockDetectionStatus,
    evidence: DeadlockDecisionEvidence,
) -> DeadlockDecision {
    match status {
        DeadlockDetectionStatus::NoCycle => DeadlockDecision::NoCycle { evidence },
        DeadlockDetectionStatus::CycleFound {
            victim,
            cycle_participants,
        } => DeadlockDecision::VictimSuggested {
            victim,
            cycle_participants,
            evidence,
        },
        DeadlockDetectionStatus::TimedOut {
            reason,
            elapsed,
            timeout,
        } => DeadlockDecision::TimedOut {
            reason,
            elapsed,
            timeout,
            evidence,
        },
        DeadlockDetectionStatus::Deferred { reason } => {
            DeadlockDecision::Deferred { reason, evidence }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock_manager::{LockAcquireStatus, LockMode, LockResource};
    use crate::{TransactionStatus, TransactionStatusTable};

    #[test]
    fn graph_edge_insertion_is_deterministic_and_idempotent() {
        let mut graph = WaitForGraph::new();

        assert!(
            graph
                .insert_edge(TransactionId::new(3), TransactionId::new(2))
                .unwrap()
        );
        assert!(
            graph
                .insert_edge(TransactionId::new(1), TransactionId::new(4))
                .unwrap()
        );
        assert!(
            graph
                .insert_edge(TransactionId::new(1), TransactionId::new(2))
                .unwrap()
        );
        assert!(
            !graph
                .insert_edge(TransactionId::new(1), TransactionId::new(2))
                .unwrap()
        );

        assert_eq!(
            graph.edges(),
            vec![
                (TransactionId::new(1), TransactionId::new(2)),
                (TransactionId::new(1), TransactionId::new(4)),
                (TransactionId::new(3), TransactionId::new(2)),
            ]
        );
        assert_eq!(graph.edge_count(), 3);
    }

    #[test]
    fn graph_from_lock_manager_adds_exclusive_waiter_edge_to_shared_holder() {
        let manager = LockManager::new();
        let resource = LockResource::row(1, 2, 3).unwrap();

        assert_eq!(
            manager
                .acquire(TransactionId::new(1), resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert!(matches!(
            manager
                .acquire(TransactionId::new(2), resource, LockMode::Exclusive)
                .unwrap(),
            LockAcquireStatus::Waiting { sequence: 1, .. }
        ));

        let graph = WaitForGraph::from_lock_manager(&manager).unwrap();

        assert_eq!(
            graph.edges(),
            vec![(TransactionId::new(2), TransactionId::new(1))]
        );
    }

    #[test]
    fn graph_from_lock_manager_omits_edge_for_compatible_waiter() {
        let manager = LockManager::new();
        let resource = LockResource::table(1, 2).unwrap();

        manager
            .record_holder(resource, TransactionId::new(1), LockMode::Shared)
            .unwrap();
        manager
            .enqueue_waiter(resource, TransactionId::new(2), LockMode::Shared)
            .unwrap();

        let graph = WaitForGraph::from_lock_manager(&manager).unwrap();

        assert!(graph.is_empty());
    }

    #[test]
    fn graph_from_lock_manager_upgrade_waiter_excludes_self_holder() {
        let manager = LockManager::new();
        let resource = LockResource::row(1, 2, 3).unwrap();
        let upgrading_tx = TransactionId::new(7);
        let blocking_tx = TransactionId::new(8);

        assert_eq!(
            manager
                .acquire(upgrading_tx, resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert_eq!(
            manager
                .acquire(blocking_tx, resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert!(matches!(
            manager
                .acquire(upgrading_tx, resource, LockMode::Exclusive)
                .unwrap(),
            LockAcquireStatus::WaitingUpgrade { sequence: 1, .. }
        ));

        let graph = WaitForGraph::from_lock_manager(&manager).unwrap();

        assert_eq!(graph.edges(), vec![(upgrading_tx, blocking_tx)]);
    }

    #[test]
    fn graph_from_lock_manager_combines_multi_resource_edges_deterministically() {
        let manager = LockManager::new();
        let first_resource = LockResource::table(1, 2).unwrap();
        let second_resource = LockResource::row(1, 2, 3).unwrap();

        assert_eq!(
            manager
                .acquire(TransactionId::new(1), first_resource, LockMode::Exclusive)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert!(matches!(
            manager
                .acquire(TransactionId::new(2), first_resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Waiting { sequence: 1, .. }
        ));
        assert_eq!(
            manager
                .acquire(TransactionId::new(3), second_resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert_eq!(
            manager
                .acquire(TransactionId::new(5), second_resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert!(matches!(
            manager
                .acquire(TransactionId::new(4), second_resource, LockMode::Exclusive)
                .unwrap(),
            LockAcquireStatus::Waiting { sequence: 2, .. }
        ));

        let graph = WaitForGraph::from_lock_manager(&manager).unwrap();

        assert_eq!(
            graph.edges(),
            vec![
                (TransactionId::new(2), TransactionId::new(1)),
                (TransactionId::new(4), TransactionId::new(3)),
                (TransactionId::new(4), TransactionId::new(5)),
            ]
        );
    }

    #[test]
    fn graph_from_empty_lock_manager_is_empty() {
        let manager = LockManager::new();

        let graph = WaitForGraph::from_lock_manager(&manager).unwrap();

        assert!(graph.is_empty());
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn graph_edge_removal_removes_empty_waiter_entries() {
        let mut graph = WaitForGraph::new();

        graph
            .insert_edge(TransactionId::new(1), TransactionId::new(2))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(1), TransactionId::new(3))
            .unwrap();

        assert!(
            graph
                .remove_edge(TransactionId::new(1), TransactionId::new(2))
                .unwrap()
        );
        assert_eq!(
            graph.blockers_for(TransactionId::new(1)).unwrap(),
            vec![TransactionId::new(3)]
        );
        assert!(
            graph
                .remove_edge(TransactionId::new(1), TransactionId::new(3))
                .unwrap()
        );
        assert!(graph.is_empty());
        assert!(
            !graph
                .remove_edge(TransactionId::new(1), TransactionId::new(3))
                .unwrap()
        );
    }

    #[test]
    fn graph_remove_transaction_removes_incoming_and_outgoing_edges() {
        let mut graph = WaitForGraph::new();

        graph
            .insert_edge(TransactionId::new(1), TransactionId::new(2))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(2), TransactionId::new(3))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(4), TransactionId::new(2))
            .unwrap();

        assert!(graph.remove_transaction(TransactionId::new(2)).unwrap());
        assert_eq!(graph.edges(), Vec::<(TransactionId, TransactionId)>::new());
    }

    #[test]
    fn zero_transaction_ids_are_rejected() {
        let mut graph = WaitForGraph::new();

        assert!(
            graph
                .insert_edge(TransactionId::new(0), TransactionId::new(1))
                .is_err()
        );
        assert!(
            graph
                .insert_edge(TransactionId::new(1), TransactionId::new(0))
                .is_err()
        );
        assert!(graph.remove_transaction(TransactionId::new(0)).is_err());
        assert!(
            DeadlockVictim::from_cycle(
                vec![TransactionId::new(1), TransactionId::new(0)],
                DeadlockVictimPolicy::YoungestTransactionId,
            )
            .is_err()
        );
    }

    #[test]
    fn victim_construction_is_deterministic() {
        let victim = DeadlockVictim::from_cycle(
            vec![
                TransactionId::new(5),
                TransactionId::new(2),
                TransactionId::new(9),
                TransactionId::new(5),
            ],
            DeadlockVictimPolicy::YoungestTransactionId,
        )
        .unwrap();

        assert_eq!(victim.tx_id, TransactionId::new(9));
        assert_eq!(
            victim.cycle_participants,
            vec![
                TransactionId::new(2),
                TransactionId::new(5),
                TransactionId::new(9),
            ]
        );

        let oldest = DeadlockVictim::from_cycle(
            vec![TransactionId::new(5), TransactionId::new(2)],
            DeadlockVictimPolicy::OldestTransactionId,
        )
        .unwrap();
        assert_eq!(oldest.tx_id, TransactionId::new(2));

        let mut metadata = DeadlockTransactionMetadataTable::new();
        metadata
            .register_start_order(TransactionId::new(5), 1)
            .unwrap();
        metadata
            .register_start_order(TransactionId::new(2), 3)
            .unwrap();
        let youngest = DeadlockVictim::from_cycle_with_transaction_metadata(
            vec![TransactionId::new(5), TransactionId::new(2)],
            DeadlockVictimPolicy::YoungestTransactionStartOrder,
            &metadata,
        )
        .unwrap();
        assert_eq!(youngest.tx_id, TransactionId::new(2));
    }

    #[test]
    fn default_policy_is_valid_and_timeout_is_bounded() {
        assert!(DeadlockPolicy::default().validate().is_ok());
        assert_eq!(DEFAULT_DEADLOCK_TIMEOUT, Duration::from_millis(500));
        assert_eq!(
            DeadlockPolicy::default().detection_timeout,
            Duration::from_millis(500)
        );
        assert!(
            DeadlockPolicy::new(
                Duration::from_millis(500),
                DeadlockVictimPolicy::YoungestTransactionId,
            )
            .is_ok()
        );
        assert!(
            DeadlockPolicy::new(Duration::ZERO, DeadlockVictimPolicy::YoungestTransactionId)
                .is_err()
        );
        assert!(
            DeadlockPolicy::new(
                MAX_DEADLOCK_TIMEOUT + Duration::from_nanos(1),
                DeadlockVictimPolicy::YoungestTransactionId,
            )
            .is_err()
        );
    }

    #[test]
    fn deadline_rejects_zero_and_too_large_timeouts() {
        let clock = ManualDeadlockClock::new();

        assert!(DeadlockDetectionDeadline::new(clock.now(), Duration::ZERO).is_err());
        assert!(
            DeadlockDetectionDeadline::new(
                clock.now(),
                MAX_DEADLOCK_TIMEOUT + Duration::from_nanos(1),
            )
            .is_err()
        );
    }

    #[test]
    fn manual_deadlock_clock_is_deterministic() {
        let mut clock = ManualDeadlockClock::at(Duration::from_secs(2));
        let policy = DeadlockPolicy::new(
            Duration::from_millis(500),
            DeadlockVictimPolicy::YoungestTransactionId,
        )
        .unwrap();
        let deadline = DeadlockDetectionDeadline::from_policy(policy, &clock).unwrap();

        assert_eq!(
            deadline.started_at().elapsed_since_clock_start(),
            Duration::from_secs(2)
        );
        assert!(!deadline.is_expired(&clock));

        clock.advance(Duration::from_millis(499)).unwrap();
        assert!(!deadline.is_expired(&clock));

        clock.advance(Duration::from_millis(1)).unwrap();
        assert!(deadline.is_expired(&clock));
    }

    #[test]
    fn detector_reports_no_cycle_for_empty_graph() {
        let detector = DeadlockDetector::new(DeadlockPolicy::default()).unwrap();
        let graph = WaitForGraph::new();

        assert_eq!(
            detector.detect(&graph).unwrap(),
            DeadlockDetectionStatus::NoCycle
        );
    }

    #[test]
    fn deadline_aware_detection_before_deadline_finds_cycle() {
        let detector = DeadlockDetector::new(
            DeadlockPolicy::new(
                Duration::from_millis(500),
                DeadlockVictimPolicy::YoungestTransactionId,
            )
            .unwrap(),
        )
        .unwrap();
        let mut clock = ManualDeadlockClock::new();
        let deadline = DeadlockDetectionDeadline::from_policy(detector.policy(), &clock).unwrap();
        let mut graph = WaitForGraph::new();

        graph
            .insert_edge(TransactionId::new(1), TransactionId::new(2))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(2), TransactionId::new(1))
            .unwrap();
        let edges_before = graph.edges();
        clock.advance(Duration::from_millis(499)).unwrap();

        assert_cycle(
            detector
                .detect_with_deadline(&graph, deadline, &clock)
                .unwrap(),
            vec![TransactionId::new(1), TransactionId::new(2)],
            TransactionId::new(2),
            DeadlockVictimPolicy::YoungestTransactionId,
        );
        assert_eq!(graph.edges(), edges_before);
    }

    #[test]
    fn expired_deadline_returns_explicit_timeout_status_without_graph_mutation() {
        let detector = DeadlockDetector::new(
            DeadlockPolicy::new(
                Duration::from_millis(500),
                DeadlockVictimPolicy::YoungestTransactionId,
            )
            .unwrap(),
        )
        .unwrap();
        let mut clock = ManualDeadlockClock::new();
        let deadline = DeadlockDetectionDeadline::from_policy(detector.policy(), &clock).unwrap();
        let mut graph = WaitForGraph::new();

        graph
            .insert_edge(TransactionId::new(1), TransactionId::new(2))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(2), TransactionId::new(1))
            .unwrap();
        let edges_before = graph.edges();
        clock.advance(Duration::from_millis(500)).unwrap();

        assert_eq!(
            detector
                .detect_with_deadline(&graph, deadline, &clock)
                .unwrap(),
            DeadlockDetectionStatus::TimedOut {
                reason: DEADLOCK_DETECTION_TIMEOUT_REASON,
                elapsed: Duration::from_millis(500),
                timeout: Duration::from_millis(500),
            }
        );
        assert_eq!(graph.edges(), edges_before);
    }

    #[test]
    fn detector_reports_no_cycle_for_non_empty_acyclic_graph() {
        let detector = DeadlockDetector::default();
        let mut graph = WaitForGraph::new();

        graph
            .insert_edge(TransactionId::new(1), TransactionId::new(2))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(2), TransactionId::new(3))
            .unwrap();

        assert_eq!(
            detector.detect(&graph).unwrap(),
            DeadlockDetectionStatus::NoCycle
        );
    }

    #[test]
    fn detector_finds_simple_two_node_cycle() {
        let detector = DeadlockDetector::default();
        let mut graph = WaitForGraph::new();
        let metadata = metadata_table(&[(1, 1), (2, 2)]);

        graph
            .insert_edge(TransactionId::new(1), TransactionId::new(2))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(2), TransactionId::new(1))
            .unwrap();

        assert_cycle(
            detector
                .detect_with_transaction_metadata(&graph, &metadata)
                .unwrap(),
            vec![TransactionId::new(1), TransactionId::new(2)],
            TransactionId::new(2),
            DeadlockVictimPolicy::YoungestTransactionStartOrder,
        );
    }

    #[test]
    fn detector_finds_three_node_cycle() {
        let detector = DeadlockDetector::default();
        let mut graph = WaitForGraph::new();
        let metadata = metadata_table(&[(1, 1), (2, 2), (3, 3)]);

        graph
            .insert_edge(TransactionId::new(3), TransactionId::new(1))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(1), TransactionId::new(2))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(2), TransactionId::new(3))
            .unwrap();

        assert_cycle(
            detector
                .detect_with_transaction_metadata(&graph, &metadata)
                .unwrap(),
            vec![
                TransactionId::new(1),
                TransactionId::new(2),
                TransactionId::new(3),
            ],
            TransactionId::new(3),
            DeadlockVictimPolicy::YoungestTransactionStartOrder,
        );
    }

    #[test]
    fn detector_selects_youngest_transaction_by_start_order_in_cycle() {
        let detector = DeadlockDetector::default();
        let mut graph = WaitForGraph::new();
        let metadata = metadata_table(&[(10, 3), (20, 1)]);

        graph
            .insert_edge(TransactionId::new(20), TransactionId::new(10))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(10), TransactionId::new(20))
            .unwrap();

        assert_cycle(
            detector
                .detect_with_transaction_metadata(&graph, &metadata)
                .unwrap(),
            vec![TransactionId::new(10), TransactionId::new(20)],
            TransactionId::new(10),
            DeadlockVictimPolicy::YoungestTransactionStartOrder,
        );
    }

    #[test]
    fn detector_ties_youngest_transaction_policy_by_tx_id_deterministically() {
        let detector = DeadlockDetector::default();
        let mut graph = WaitForGraph::new();
        let metadata = metadata_table(&[(10, 7), (20, 7), (30, 7)]);

        graph
            .insert_edge(TransactionId::new(10), TransactionId::new(20))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(20), TransactionId::new(30))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(30), TransactionId::new(10))
            .unwrap();

        assert_cycle(
            detector
                .detect_with_transaction_metadata(&graph, &metadata)
                .unwrap(),
            vec![
                TransactionId::new(10),
                TransactionId::new(20),
                TransactionId::new(30),
            ],
            TransactionId::new(30),
            DeadlockVictimPolicy::YoungestTransactionStartOrder,
        );
    }

    #[test]
    fn detector_reports_deferred_when_youngest_metadata_is_missing() {
        let detector = DeadlockDetector::default();
        let mut graph = WaitForGraph::new();
        let metadata = metadata_table(&[(10, 1)]);

        graph
            .insert_edge(TransactionId::new(10), TransactionId::new(20))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(20), TransactionId::new(10))
            .unwrap();

        assert_eq!(
            detector
                .detect_with_transaction_metadata(&graph, &metadata)
                .unwrap(),
            DeadlockDetectionStatus::Deferred {
                reason: MISSING_TRANSACTION_ORDERING_METADATA_REASON
            }
        );
        assert_eq!(
            detector.detect(&graph).unwrap(),
            DeadlockDetectionStatus::Deferred {
                reason: MISSING_TRANSACTION_ORDERING_METADATA_REASON
            }
        );
    }

    #[test]
    fn detector_still_supports_deterministic_id_based_and_oldest_victims() {
        let mut graph = WaitForGraph::new();

        graph
            .insert_edge(TransactionId::new(20), TransactionId::new(10))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(10), TransactionId::new(20))
            .unwrap();

        let youngest_detector = DeadlockDetector::new(
            DeadlockPolicy::new(
                DEFAULT_DEADLOCK_TIMEOUT,
                DeadlockVictimPolicy::YoungestTransactionId,
            )
            .unwrap(),
        )
        .unwrap();
        let oldest_detector = DeadlockDetector::new(
            DeadlockPolicy::new(
                DEFAULT_DEADLOCK_TIMEOUT,
                DeadlockVictimPolicy::OldestTransactionId,
            )
            .unwrap(),
        )
        .unwrap();

        assert_cycle(
            youngest_detector.detect(&graph).unwrap(),
            vec![TransactionId::new(10), TransactionId::new(20)],
            TransactionId::new(20),
            DeadlockVictimPolicy::YoungestTransactionId,
        );
        assert_cycle(
            oldest_detector.detect(&graph).unwrap(),
            vec![TransactionId::new(10), TransactionId::new(20)],
            TransactionId::new(10),
            DeadlockVictimPolicy::OldestTransactionId,
        );
    }

    #[test]
    fn detector_reports_deterministic_first_cycle_when_multiple_cycles_exist() {
        let detector = DeadlockDetector::default();
        let mut graph = WaitForGraph::new();
        let metadata = metadata_table(&[(1, 1), (2, 2), (3, 3), (4, 4), (5, 5)]);

        graph
            .insert_edge(TransactionId::new(5), TransactionId::new(4))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(1), TransactionId::new(3))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(1), TransactionId::new(2))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(3), TransactionId::new(1))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(4), TransactionId::new(5))
            .unwrap();
        graph
            .insert_edge(TransactionId::new(2), TransactionId::new(1))
            .unwrap();

        assert_cycle(
            detector
                .detect_with_transaction_metadata(&graph, &metadata)
                .unwrap(),
            vec![TransactionId::new(1), TransactionId::new(2)],
            TransactionId::new(2),
            DeadlockVictimPolicy::YoungestTransactionStartOrder,
        );
    }

    #[test]
    fn detector_finds_cycle_derived_from_lock_manager_snapshot() {
        let detector = DeadlockDetector::default();
        let manager = LockManager::new();
        let first_resource = LockResource::row(1, 2, 3).unwrap();
        let second_resource = LockResource::row(1, 2, 4).unwrap();
        let first_tx = TransactionId::new(1);
        let second_tx = TransactionId::new(2);
        let metadata = metadata_table(&[(1, 1), (2, 2)]);

        assert_eq!(
            manager
                .acquire(first_tx, first_resource, LockMode::Exclusive)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert_eq!(
            manager
                .acquire(second_tx, second_resource, LockMode::Exclusive)
                .unwrap(),
            LockAcquireStatus::Granted
        );
        assert!(matches!(
            manager
                .acquire(second_tx, first_resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Waiting { sequence: 1, .. }
        ));
        assert!(matches!(
            manager
                .acquire(first_tx, second_resource, LockMode::Shared)
                .unwrap(),
            LockAcquireStatus::Waiting { sequence: 2, .. }
        ));

        let graph = WaitForGraph::from_lock_manager(&manager).unwrap();

        assert_eq!(
            graph.edges(),
            vec![(first_tx, second_tx), (second_tx, first_tx)]
        );
        assert_cycle(
            detector
                .detect_with_transaction_metadata(&graph, &metadata)
                .unwrap(),
            vec![first_tx, second_tx],
            second_tx,
            DeadlockVictimPolicy::YoungestTransactionStartOrder,
        );
    }

    #[test]
    fn youngest_policy_does_not_mutate_lock_manager_graph_or_metadata() {
        let detector = DeadlockDetector::default();
        let manager = LockManager::new();
        let first_resource = LockResource::row(1, 2, 3).unwrap();
        let second_resource = LockResource::row(1, 2, 4).unwrap();
        let first_tx = TransactionId::new(1);
        let second_tx = TransactionId::new(2);
        let metadata = metadata_table(&[(1, 2), (2, 1)]);
        let transaction_statuses = TransactionStatusTable::new();
        transaction_statuses
            .record(first_tx, TransactionStatus::InFlight)
            .unwrap();
        transaction_statuses
            .record(second_tx, TransactionStatus::InFlight)
            .unwrap();

        manager
            .acquire(first_tx, first_resource, LockMode::Exclusive)
            .unwrap();
        manager
            .acquire(second_tx, second_resource, LockMode::Exclusive)
            .unwrap();
        manager
            .acquire(second_tx, first_resource, LockMode::Shared)
            .unwrap();
        manager
            .acquire(first_tx, second_resource, LockMode::Shared)
            .unwrap();

        let snapshot_before = manager.snapshot().unwrap();
        let graph = WaitForGraph::from_lock_manager(&manager).unwrap();
        let graph_before = graph.clone();
        let metadata_before = metadata.clone();
        let first_status_before = transaction_statuses.status(first_tx);
        let second_status_before = transaction_statuses.status(second_tx);

        assert_cycle(
            detector
                .detect_with_transaction_metadata(&graph, &metadata)
                .unwrap(),
            vec![first_tx, second_tx],
            first_tx,
            DeadlockVictimPolicy::YoungestTransactionStartOrder,
        );

        assert_eq!(manager.snapshot().unwrap(), snapshot_before);
        assert_eq!(graph, graph_before);
        assert_eq!(metadata, metadata_before);
        assert_eq!(transaction_statuses.status(first_tx), first_status_before);
        assert_eq!(transaction_statuses.status(second_tx), second_status_before);
    }

    #[test]
    fn decision_from_waiting_lock_cycle_suggests_victim_with_snapshot_evidence() {
        let manager = LockManager::new();
        let first_resource = LockResource::row(1, 2, 3).unwrap();
        let second_resource = LockResource::row(1, 2, 4).unwrap();
        let first_tx = TransactionId::new(1);
        let second_tx = TransactionId::new(2);
        let metadata = metadata_table(&[(1, 1), (2, 2)]);

        manager
            .acquire(first_tx, first_resource, LockMode::Exclusive)
            .unwrap();
        manager
            .acquire(second_tx, second_resource, LockMode::Exclusive)
            .unwrap();
        manager
            .acquire(second_tx, first_resource, LockMode::Shared)
            .unwrap();
        manager
            .acquire(first_tx, second_resource, LockMode::Shared)
            .unwrap();
        let snapshot_before = manager.snapshot().unwrap();

        let decision =
            decide_deadlock_from_lock_manager(&manager, DeadlockPolicy::default(), Some(&metadata))
                .unwrap();

        let DeadlockDecision::VictimSuggested {
            victim,
            cycle_participants,
            evidence,
        } = decision
        else {
            panic!("expected victim suggestion decision");
        };
        assert_eq!(victim.tx_id, second_tx);
        assert_eq!(
            victim.victim_policy,
            DeadlockVictimPolicy::YoungestTransactionStartOrder
        );
        assert_eq!(cycle_participants, vec![first_tx, second_tx]);
        assert_eq!(evidence.lock_snapshot_resource_count, 2);
        assert_eq!(
            evidence.wait_for_edges,
            vec![(first_tx, second_tx), (second_tx, first_tx)]
        );
        assert_eq!(manager.snapshot().unwrap(), snapshot_before);
    }

    #[test]
    fn decision_from_acyclic_waiting_locks_returns_no_cycle() {
        let manager = LockManager::new();
        let resource = LockResource::row(1, 2, 3).unwrap();
        let holder = TransactionId::new(1);
        let waiter = TransactionId::new(2);

        manager
            .acquire(holder, resource, LockMode::Exclusive)
            .unwrap();
        manager.acquire(waiter, resource, LockMode::Shared).unwrap();

        let decision = decide_deadlock_from_lock_manager(
            &manager,
            DeadlockPolicy::new(
                DEFAULT_DEADLOCK_TIMEOUT,
                DeadlockVictimPolicy::YoungestTransactionId,
            )
            .unwrap(),
            None,
        )
        .unwrap();

        let DeadlockDecision::NoCycle { evidence } = decision else {
            panic!("expected no-cycle decision");
        };
        assert_eq!(evidence.lock_snapshot_resource_count, 1);
        assert_eq!(evidence.wait_for_edges, vec![(waiter, holder)]);
    }

    #[test]
    fn decision_defers_when_youngest_policy_metadata_is_missing() {
        let manager = LockManager::new();
        let first_resource = LockResource::row(1, 2, 3).unwrap();
        let second_resource = LockResource::row(1, 2, 4).unwrap();
        let first_tx = TransactionId::new(1);
        let second_tx = TransactionId::new(2);

        manager
            .acquire(first_tx, first_resource, LockMode::Exclusive)
            .unwrap();
        manager
            .acquire(second_tx, second_resource, LockMode::Exclusive)
            .unwrap();
        manager
            .acquire(second_tx, first_resource, LockMode::Shared)
            .unwrap();
        manager
            .acquire(first_tx, second_resource, LockMode::Shared)
            .unwrap();

        let decision =
            decide_deadlock_from_lock_manager(&manager, DeadlockPolicy::default(), None).unwrap();

        let DeadlockDecision::Deferred { reason, evidence } = decision else {
            panic!("expected deferred decision");
        };
        assert_eq!(reason, MISSING_TRANSACTION_ORDERING_METADATA_REASON);
        assert_eq!(evidence.wait_for_edge_count(), 2);
    }

    #[test]
    fn decision_times_out_when_deadline_expired() {
        let manager = LockManager::new();
        let resource = LockResource::row(1, 2, 3).unwrap();
        let holder = TransactionId::new(1);
        let waiter = TransactionId::new(2);
        let policy = DeadlockPolicy::new(
            Duration::from_millis(5),
            DeadlockVictimPolicy::YoungestTransactionId,
        )
        .unwrap();
        let mut clock = ManualDeadlockClock::new();
        let deadline = DeadlockDetectionDeadline::from_policy(policy, &clock).unwrap();

        manager
            .acquire(holder, resource, LockMode::Exclusive)
            .unwrap();
        manager.acquire(waiter, resource, LockMode::Shared).unwrap();
        clock.advance(Duration::from_millis(5)).unwrap();

        let decision = decide_deadlock_from_lock_manager_with_deadline(
            &manager, policy, None, deadline, &clock,
        )
        .unwrap();

        assert_eq!(
            decision,
            DeadlockDecision::TimedOut {
                reason: DEADLOCK_DETECTION_TIMEOUT_REASON,
                elapsed: Duration::from_millis(5),
                timeout: Duration::from_millis(5),
                evidence: DeadlockDecisionEvidence {
                    lock_snapshot_resource_count: 1,
                    wait_for_edges: vec![(waiter, holder)],
                },
            }
        );
    }

    #[test]
    fn typed_decision_is_consumable_by_coordinator_without_direct_abort() {
        fn coordinator_pick_victim(decision: &DeadlockDecision) -> Option<TransactionId> {
            match decision {
                DeadlockDecision::VictimSuggested { victim, .. } => Some(victim.tx_id),
                DeadlockDecision::NoCycle { .. }
                | DeadlockDecision::Deferred { .. }
                | DeadlockDecision::TimedOut { .. } => None,
            }
        }

        let manager = LockManager::new();
        let first_resource = LockResource::row(1, 2, 3).unwrap();
        let second_resource = LockResource::row(1, 2, 4).unwrap();
        let first_tx = TransactionId::new(1);
        let second_tx = TransactionId::new(2);
        let metadata = metadata_table(&[(1, 1), (2, 2)]);
        let transaction_statuses = TransactionStatusTable::new();
        transaction_statuses
            .record(first_tx, TransactionStatus::InFlight)
            .unwrap();
        transaction_statuses
            .record(second_tx, TransactionStatus::InFlight)
            .unwrap();
        let first_status_before = transaction_statuses.status(first_tx);
        let second_status_before = transaction_statuses.status(second_tx);

        manager
            .acquire(first_tx, first_resource, LockMode::Exclusive)
            .unwrap();
        manager
            .acquire(second_tx, second_resource, LockMode::Exclusive)
            .unwrap();
        manager
            .acquire(second_tx, first_resource, LockMode::Shared)
            .unwrap();
        manager
            .acquire(first_tx, second_resource, LockMode::Shared)
            .unwrap();

        let decision =
            decide_deadlock_from_lock_manager(&manager, DeadlockPolicy::default(), Some(&metadata))
                .unwrap();

        assert_eq!(coordinator_pick_victim(&decision), Some(second_tx));
        assert_eq!(transaction_statuses.status(first_tx), first_status_before);
        assert_eq!(transaction_statuses.status(second_tx), second_status_before);
    }

    fn assert_cycle(
        status: DeadlockDetectionStatus,
        expected_participants: Vec<TransactionId>,
        expected_victim: TransactionId,
        expected_policy: DeadlockVictimPolicy,
    ) {
        let DeadlockDetectionStatus::CycleFound {
            victim,
            cycle_participants,
        } = status
        else {
            panic!("expected cycle found status");
        };

        assert_eq!(cycle_participants, expected_participants);
        assert_eq!(victim.tx_id, expected_victim);
        assert_eq!(victim.victim_policy, expected_policy);
        assert_eq!(victim.cycle_participants, expected_participants);
    }

    fn metadata_table(entries: &[(u64, u64)]) -> DeadlockTransactionMetadataTable {
        let mut metadata = DeadlockTransactionMetadataTable::new();
        for (tx_id, start_order) in entries {
            metadata
                .register_start_order(TransactionId::new(*tx_id), *start_order)
                .unwrap();
        }
        metadata
    }
}
