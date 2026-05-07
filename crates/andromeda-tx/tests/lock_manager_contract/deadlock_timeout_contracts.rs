use super::fixtures::{assert_granted, row_resource};
use super::*;

#[test]
fn deadlock_decision_projects_trace_evidence_without_abort_side_effect() {
    let manager = LockManager::new();
    let first_resource = row_resource(74, 704);
    let second_resource = row_resource(74, 705);
    let first_tx = TransactionId::new(77);
    let second_tx = TransactionId::new(78);

    assert_granted(&manager, first_tx, first_resource, LockMode::Exclusive);
    assert_granted(&manager, second_tx, second_resource, LockMode::Exclusive);
    assert!(matches!(
        manager
            .acquire(first_tx, second_resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { .. }
    ));
    assert!(matches!(
        manager
            .acquire(second_tx, first_resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { .. }
    ));

    let policy = DeadlockPolicy::new(
        andromeda_tx::DEFAULT_DEADLOCK_TIMEOUT,
        DeadlockVictimPolicy::YoungestTransactionId,
    )
    .unwrap();
    let decision = decide_deadlock_from_lock_manager(&manager, policy, None).unwrap();
    let trace = decision.to_trace();

    assert_eq!(trace.outcome, DeadlockDecisionTraceOutcome::VictimSuggested);
    assert_eq!(trace.tx_id, Some(second_tx));
    assert_eq!(trace.cycle_participants, vec![first_tx, second_tx]);
    assert_eq!(
        trace.wait_for_edges,
        vec![(first_tx, second_tx), (second_tx, first_tx)]
    );
}
