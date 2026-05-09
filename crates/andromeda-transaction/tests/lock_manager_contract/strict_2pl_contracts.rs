use super::fixtures::{assert_transaction_error, assert_tx_lock_granted, holder, row_resource};
use super::*;

#[test]
fn locking_protocol_growing_phase_multiple_acquires() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let tx = transactions.begin().unwrap();
    let res1 = row_resource(100, 1000);
    let res2 = row_resource(100, 1001);

    assert_tx_lock_granted(&transactions, &locks, tx, res1, LockMode::Shared);
    assert_tx_lock_granted(&transactions, &locks, tx, res2, LockMode::Exclusive);

    let snap = locks.snapshot().unwrap();
    assert_eq!(snap.len(), 2);
}

#[test]
fn locking_protocol_transition_from_active_to_committing() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let tx = transactions.begin().unwrap();
    let resource = row_resource(101, 1010);

    assert_tx_lock_granted(&transactions, &locks, tx, resource, LockMode::Shared);

    transactions.request_commit(tx).unwrap();

    let snap = transactions.snapshot(tx).unwrap().unwrap();
    assert_eq!(snap.state_machine.state(), TransactionState::Committing);
}

#[test]
fn locking_protocol_shrinking_phase_release_locks() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let tx = transactions.begin().unwrap();
    let res1 = row_resource(102, 1020);
    let res2 = row_resource(102, 1021);

    assert_tx_lock_granted(&transactions, &locks, tx, res1, LockMode::Shared);
    assert_tx_lock_granted(&transactions, &locks, tx, res2, LockMode::Exclusive);

    transactions.request_commit(tx).unwrap();

    assert!(transactions.release_lock(&locks, tx, res1).unwrap());
    assert!(transactions.release_lock(&locks, tx, res2).unwrap());

    let snap = locks.snapshot().unwrap();
    assert_eq!(snap.len(), 0);
}

#[test]
fn locking_protocol_terminal_cleanup_with_release_all() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let tx = transactions.begin().unwrap();
    let resource = row_resource(103, 1030);

    assert_tx_lock_granted(&transactions, &locks, tx, resource, LockMode::Shared);

    transactions.request_commit(tx).unwrap();
    transactions.commit_durable(tx, 1).unwrap();

    let summary = transactions.release_all_locks(&locks, tx).unwrap();
    assert!(summary.removed_any());

    let snap = locks.snapshot().unwrap();
    assert_eq!(snap.len(), 0);
}

#[test]
fn locking_protocol_rollback_path_releases_locks() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let tx = transactions.begin().unwrap();
    let resource = row_resource(104, 1040);

    assert_tx_lock_granted(&transactions, &locks, tx, resource, LockMode::Shared);

    transactions.request_rollback(tx).unwrap();
    transactions.rollback_durable(tx, 1).unwrap();

    let summary = transactions.release_all_locks(&locks, tx).unwrap();
    assert!(summary.removed_any());

    let snap = locks.snapshot().unwrap();
    assert_eq!(snap.len(), 0);
}

#[test]
fn locking_protocol_prevents_acquire_in_inflight_state_other_than_active() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let tx = transactions.begin().unwrap();
    let resource = row_resource(105, 1050);

    assert_tx_lock_granted(&transactions, &locks, tx, resource, LockMode::Shared);

    transactions.request_commit(tx).unwrap();

    let snap = transactions.snapshot(tx).unwrap().unwrap();
    assert_eq!(snap.state_machine.state(), TransactionState::Committing);

    assert_transaction_error(transactions.acquire_lock(
        &locks,
        tx,
        row_resource(105, 1051),
        LockMode::Shared,
    ));
}

#[test]
fn locking_protocol_prevents_single_release_before_shrinking_phase() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let tx = transactions.begin().unwrap();
    let resource = row_resource(106, 1060);

    assert_tx_lock_granted(&transactions, &locks, tx, resource, LockMode::Shared);

    assert_transaction_error(transactions.release_lock(&locks, tx, resource));

    let entry = locks.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders, vec![holder(tx, LockMode::Shared)]);
}
