use super::support::*;

#[test]
fn test_acquire_only_in_active_state() {
    let tx_mgr = TransactionManager::new();
    let lock_mgr = LockManager::new();
    let tx_id = tx_mgr.begin().unwrap();
    let coordinator = tx_mgr.lock_coordinator(&lock_mgr);

    let acquire_result = coordinator.acquire(tx_id, row_resource(1), LockMode::Shared);

    assert_eq!(acquire_result.unwrap(), LockAcquireStatus::Granted);
}

#[test]
fn test_acquire_validator_rejects_non_growing_states() {
    for state in [
        TransactionState::Created,
        TransactionState::Committing,
        TransactionState::Committed,
        TransactionState::RolledBack,
        TransactionState::Disposed,
        TransactionState::Poisoned,
        TransactionState::Failed,
    ] {
        assert_acquire_rejected(state);
    }
}

#[test]
fn test_coordinator_rejects_acquire_after_entering_committing() {
    let tx_mgr = TransactionManager::new();
    let lock_mgr = LockManager::new();

    let tx_id = tx_mgr.begin().unwrap();
    let coordinator = tx_mgr.lock_coordinator(&lock_mgr);
    let resource = row_resource(1);

    coordinator
        .acquire(tx_id, resource, LockMode::Shared)
        .unwrap();
    tx_mgr.request_commit(tx_id).unwrap();
    assert_tx_state(&tx_mgr, tx_id, TransactionState::Committing);

    let err = coordinator
        .acquire(tx_id, row_resource(2), LockMode::Shared)
        .expect_err("committing transaction is already in the shrinking phase");
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

#[test]
fn test_state_allows_acquire_committing_is_false() {
    let state = TransactionState::Committing;

    assert!(!TwoPhaseLocksValidator::state_allows_acquire(state));
    assert_operation_rejected(state, TwoPhaseOperation::Acquire);
}
