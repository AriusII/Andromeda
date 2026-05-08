use super::helpers::two_resource_deadlock_fixture;
use super::*;

#[test]
fn decision_from_waiting_lock_cycle_suggests_victim_with_snapshot_evidence() {
    let fixture = two_resource_deadlock_fixture(&[(1, 1), (2, 2)]);
    let snapshot_before = fixture.manager.snapshot().unwrap();

    let decision = decide_deadlock_from_lock_manager(
        &fixture.manager,
        DeadlockPolicy::default(),
        Some(&fixture.metadata),
    )
    .unwrap();

    let DeadlockDecision::VictimSuggested {
        victim,
        cycle_participants,
        evidence,
    } = decision
    else {
        panic!("expected victim suggestion decision");
    };
    assert_eq!(victim.tx_id, fixture.second_tx);
    assert_eq!(
        victim.victim_policy,
        DeadlockVictimPolicy::YoungestTransactionStartOrder
    );
    assert_eq!(
        cycle_participants,
        vec![fixture.first_tx, fixture.second_tx]
    );
    assert_eq!(evidence.lock_snapshot_resource_count, 2);
    assert_eq!(
        evidence.wait_for_edges,
        vec![
            (fixture.first_tx, fixture.second_tx),
            (fixture.second_tx, fixture.first_tx),
        ]
    );
    assert_eq!(fixture.manager.snapshot().unwrap(), snapshot_before);
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
    let fixture = two_resource_deadlock_fixture(&[]);

    let decision =
        decide_deadlock_from_lock_manager(&fixture.manager, DeadlockPolicy::default(), None)
            .unwrap();

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

    let decision =
        decide_deadlock_from_lock_manager_with_deadline(&manager, policy, None, deadline, &clock)
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

    let fixture = two_resource_deadlock_fixture(&[(1, 1), (2, 2)]);
    let transaction_statuses = TransactionStatusTable::new();
    transaction_statuses
        .record(fixture.first_tx, TransactionStatus::InFlight)
        .unwrap();
    transaction_statuses
        .record(fixture.second_tx, TransactionStatus::InFlight)
        .unwrap();
    let first_status_before = transaction_statuses.status(fixture.first_tx);
    let second_status_before = transaction_statuses.status(fixture.second_tx);

    let decision = decide_deadlock_from_lock_manager(
        &fixture.manager,
        DeadlockPolicy::default(),
        Some(&fixture.metadata),
    )
    .unwrap();

    assert_eq!(coordinator_pick_victim(&decision), Some(fixture.second_tx));
    assert_eq!(
        transaction_statuses.status(fixture.first_tx),
        first_status_before
    );
    assert_eq!(
        transaction_statuses.status(fixture.second_tx),
        second_status_before
    );
}
