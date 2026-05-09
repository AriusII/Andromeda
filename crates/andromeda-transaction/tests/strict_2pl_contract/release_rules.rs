use super::support::*;

#[test]
fn test_transaction_coordinator_rejects_release_in_active_state() {
    let tx_mgr = TransactionManager::new();
    let lock_mgr = LockManager::new();
    let tx_id = tx_mgr.begin().unwrap();
    let coordinator = tx_mgr.lock_coordinator(&lock_mgr);
    let resource = row_resource(20);

    assert_eq!(
        coordinator
            .acquire(tx_id, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    let err = coordinator
        .release(tx_id, resource)
        .expect_err("single-resource release must wait for shrinking phase");
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
    assert!(lock_mgr.entry(resource).unwrap().is_some());
}

#[test]
fn test_release_all_requires_terminal_status() {
    let tx_mgr = TransactionManager::new();
    let lock_mgr = LockManager::new();

    let tx_id = tx_mgr.begin().unwrap();
    let coordinator = tx_mgr.lock_coordinator(&lock_mgr);
    coordinator
        .acquire(tx_id, row_resource(1), LockMode::Shared)
        .unwrap();

    assert!(coordinator.release_all(tx_id).is_err());

    tx_mgr.request_commit(tx_id).unwrap();
    tx_mgr.commit_durable(tx_id, 1).unwrap();

    assert!(coordinator.release_all(tx_id).is_ok());
}
