use super::cycle::{CycleSearchOutcome, find_first_cycle_until};
use super::{
    DEADLOCK_DETECTION_TIMEOUT_REASON, DeadlockClock, DeadlockClockInstant,
    DeadlockDetectionDeadline, DeadlockDetectionStatus, DeadlockPolicy, DeadlockResult,
    DeadlockTransactionMetadataTable, DeadlockVictim, DeadlockVictimPolicy,
    MISSING_TRANSACTION_ORDERING_METADATA_REASON, ManualDeadlockClock, WaitForGraph,
    deadlock_error, policy,
};

/// Deadlock detector.
///
/// This type validates policy and graph inputs, then runs deterministic DFS over
/// sorted transaction identifiers and sorted wait-for blockers. It reports a
/// victim description only; it does not invoke transaction abort or rollback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
        policy::validate_deadlock_timeout(deadline.timeout())?;
        if deadline.timeout() > self.policy.detection_timeout {
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
        timeout: deadline.timeout(),
    }
}
