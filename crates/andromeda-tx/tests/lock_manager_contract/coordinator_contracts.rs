use super::fixtures::{assert_tx_lock_granted, holder, row_resource, table_resource};
use super::*;

#[test]
fn transaction_lock_coordinator_acquires_lock_for_valid_transaction() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let tx = transactions.begin().unwrap();
    let resource = row_resource(60, 600);
    let coordinator = transactions.lock_coordinator(&locks);

    assert_eq!(
        coordinator.acquire(tx, resource, LockMode::Shared).unwrap(),
        LockAcquireStatus::Granted
    );

    let entry = locks.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders, vec![holder(tx, LockMode::Shared)]);
    assert!(entry.waiters.is_empty());
}

#[test]
fn transaction_lock_coordinator_propagates_waiting_status() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let holder_tx = transactions.begin().unwrap();
    let waiter = transactions.begin().unwrap();
    let resource = table_resource(70);

    assert_tx_lock_granted(
        &transactions,
        &locks,
        holder_tx,
        resource,
        LockMode::Exclusive,
    );

    assert_eq!(
        transactions
            .acquire_lock(&locks, waiter, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting {
            sequence: 1,
            blockers: vec![holder(holder_tx, LockMode::Exclusive)],
        }
    );
}
