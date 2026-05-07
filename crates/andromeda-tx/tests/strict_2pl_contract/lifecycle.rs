use super::support::*;

#[test]
fn test_2pl_growing_phase_then_shrinking_phase() {
    let tx_mgr = TransactionManager::new();
    let lock_mgr = LockManager::new();

    let tx_id = tx_mgr.begin().unwrap();
    let coordinator = tx_mgr.lock_coordinator(&lock_mgr);
    let resource1 = row_resource(1);
    let resource2 = row_resource(2);

    assert_eq!(
        coordinator
            .acquire(tx_id, resource1, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        coordinator
            .acquire(tx_id, resource2, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    tx_mgr.request_commit(tx_id).unwrap();
    assert!(coordinator.release(tx_id, resource1).is_ok());
    assert!(coordinator.release(tx_id, resource2).is_ok());

    tx_mgr.commit_durable(tx_id, 1).unwrap();
    assert!(coordinator.release_all(tx_id).is_ok());
}

#[test]
fn test_2pl_state_transition_sequence_valid() {
    let tx_mgr = TransactionManager::new();
    let tx_id = tx_mgr.begin().unwrap();

    assert_tx_state(&tx_mgr, tx_id, TransactionState::Active);

    tx_mgr.request_commit(tx_id).unwrap();
    assert_tx_state(&tx_mgr, tx_id, TransactionState::Committing);

    tx_mgr.commit_durable(tx_id, 1).unwrap();
    assert_tx_state(&tx_mgr, tx_id, TransactionState::Committed);

    tx_mgr.dispose(tx_id).unwrap();
    assert_tx_disposed(&tx_mgr, tx_id);
}

#[test]
fn test_2pl_rollback_path_sequence() {
    let tx_mgr = TransactionManager::new();
    let tx_id = tx_mgr.begin().unwrap();

    assert_tx_state(&tx_mgr, tx_id, TransactionState::Active);

    tx_mgr.request_rollback(tx_id).unwrap();
    assert_tx_state(&tx_mgr, tx_id, TransactionState::RollingBack);

    tx_mgr.rollback_durable(tx_id, 1).unwrap();
    assert_tx_state(&tx_mgr, tx_id, TransactionState::RolledBack);

    tx_mgr.dispose(tx_id).unwrap();
    assert_tx_disposed(&tx_mgr, tx_id);
}

#[test]
fn test_no_acquire_after_release_shrinking_phase() {
    let tx_mgr = TransactionManager::new();
    let lock_mgr = LockManager::new();

    let tx_id = tx_mgr.begin().unwrap();
    let coordinator = tx_mgr.lock_coordinator(&lock_mgr);
    let resource1 = row_resource(1);
    let resource2 = row_resource(2);

    coordinator
        .acquire(tx_id, resource1, LockMode::Shared)
        .unwrap();
    coordinator
        .acquire(tx_id, resource2, LockMode::Shared)
        .unwrap();

    tx_mgr.request_commit(tx_id).unwrap();
    coordinator.release(tx_id, resource1).unwrap();

    let err = coordinator
        .acquire(tx_id, row_resource(3), LockMode::Shared)
        .expect_err("2PL forbids acquiring after entering shrinking phase");
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

#[test]
fn test_failed_transaction_must_rollback() {
    let tx_mgr = TransactionManager::new();
    let tx_id = tx_mgr.begin().unwrap();

    tx_mgr.fail(tx_id).unwrap();
    assert_tx_state(&tx_mgr, tx_id, TransactionState::Failed);
    assert_acquire_rejected(TransactionState::Failed);

    tx_mgr.request_rollback(tx_id).unwrap();
    assert_tx_state(&tx_mgr, tx_id, TransactionState::RollingBack);
}

#[test]
fn test_multiple_transactions_independent_2pl() {
    let tx_mgr = TransactionManager::new();
    let lock_mgr = LockManager::new();

    let tx1 = tx_mgr.begin().unwrap();
    let tx2 = tx_mgr.begin().unwrap();
    let coordinator = tx_mgr.lock_coordinator(&lock_mgr);

    assert!(
        coordinator
            .acquire(tx1, row_resource(1), LockMode::Shared)
            .is_ok()
    );
    assert!(
        coordinator
            .acquire(tx2, row_resource(2), LockMode::Exclusive)
            .is_ok()
    );

    tx_mgr.request_commit(tx1).unwrap();
    assert_tx_state(&tx_mgr, tx1, TransactionState::Committing);
    assert_tx_state(&tx_mgr, tx2, TransactionState::Active);

    assert_operation_rejected(TransactionState::Committing, TwoPhaseOperation::Acquire);
    assert_operation_allowed(TransactionState::Active, TwoPhaseOperation::Acquire);
}
