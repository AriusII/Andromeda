use super::support::*;

#[test]
fn test_poisoned_transaction_rejects_locks() {
    let tx_mgr = TransactionManager::new();
    let lock_mgr = LockManager::new();

    let tx_id = tx_mgr.begin().unwrap();
    let coordinator = tx_mgr.lock_coordinator(&lock_mgr);
    coordinator
        .acquire(tx_id, row_resource(1), LockMode::Shared)
        .unwrap();

    tx_mgr.poison(tx_id).unwrap();
    assert_tx_state(&tx_mgr, tx_id, TransactionState::Poisoned);
    assert_acquire_rejected(TransactionState::Poisoned);

    let err = coordinator
        .acquire(tx_id, row_resource(2), LockMode::Shared)
        .expect_err("poisoned transaction must not acquire more locks");
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

#[test]
fn test_disposed_transaction_no_operations() {
    let tx_mgr = TransactionManager::new();

    let tx_id = tx_mgr.begin().unwrap();
    tx_mgr.request_commit(tx_id).unwrap();
    tx_mgr.commit_durable(tx_id, 1).unwrap();
    tx_mgr.dispose(tx_id).unwrap();

    let disposed_state = TransactionState::Disposed;
    assert_acquire_rejected(disposed_state);
    assert_release_rejected(disposed_state);
    assert_release_all_rejected(disposed_state);
}

#[test]
fn test_2pl_validator_noop_operation() {
    assert_operation_allowed(TransactionState::Disposed, TwoPhaseOperation::NoOp);
}
