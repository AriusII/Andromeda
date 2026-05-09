use super::fixtures::{assert_granted, holder, row_resource, table_resource};
use super::*;

#[test]
fn lock_manager_grants_lock_for_valid_owner_request() {
    let locks = LockManager::new();
    let tx = TransactionId::new(1);
    let resource = row_resource(60, 600);

    assert_eq!(
        locks.acquire(tx, resource, LockMode::Shared).unwrap(),
        LockAcquireStatus::Granted
    );

    let entry = locks.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders, vec![holder(tx, LockMode::Shared)]);
    assert!(entry.waiters.is_empty());
}

#[test]
fn lock_manager_propagates_waiting_status_with_blocker_evidence() {
    let locks = LockManager::new();
    let holder_tx = TransactionId::new(2);
    let waiter = TransactionId::new(3);
    let resource = table_resource(70);

    assert_granted(&locks, holder_tx, resource, LockMode::Exclusive);

    assert_eq!(
        locks.acquire(waiter, resource, LockMode::Shared).unwrap(),
        LockAcquireStatus::Waiting {
            sequence: 1,
            blockers: vec![holder(holder_tx, LockMode::Exclusive)],
        }
    );
}
