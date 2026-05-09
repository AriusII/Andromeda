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

    assert!(
        coordinator
            .acquire(tx1, row_resource(3), LockMode::Shared)
            .is_err()
    );
    assert!(
        coordinator
            .acquire(tx2, row_resource(4), LockMode::Shared)
            .is_ok()
    );
}
