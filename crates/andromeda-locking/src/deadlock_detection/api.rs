use crate::{LockEntry, LockManager, LockResource};

use super::{
    DeadlockClock, DeadlockDecision, DeadlockDecisionEvidence, DeadlockDetectionDeadline,
    DeadlockDetector, DeadlockPolicy, DeadlockResult, DeadlockTransactionMetadataTable,
    ManualDeadlockClock, WaitForGraph, decision_from_detection_status,
};

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
