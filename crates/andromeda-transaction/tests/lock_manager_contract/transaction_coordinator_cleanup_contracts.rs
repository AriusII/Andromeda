use super::fixtures::{
    assert_transaction_error, assert_tx_lock_granted, holder, row_resource, schema_resource,
    table_resource,
};
use super::*;

#[test]
fn transaction_lock_coordinator_terminal_release_all_removes_holders_and_waiters() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let ending_tx = transactions.begin().unwrap();
    let other_tx = transactions.begin().unwrap();
    let held_resource = schema_resource(1);
    let waited_resource = row_resource(80, 800);

    assert_tx_lock_granted(
        &transactions,
        &locks,
        ending_tx,
        held_resource,
        LockMode::SchemaShared,
    );
    assert_tx_lock_granted(
        &transactions,
        &locks,
        other_tx,
        waited_resource,
        LockMode::Exclusive,
    );
    assert!(matches!(
        transactions
            .acquire_lock(&locks, ending_tx, waited_resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { .. }
    ));

    transactions.request_commit(ending_tx).unwrap();
    transactions.commit_durable(ending_tx, 88).unwrap();

    let summary = transactions.release_all_locks(&locks, ending_tx).unwrap();

    assert_eq!(
        summary,
        LockReleaseAllSummary {
            affected_resource_count: 2,
            released_holder_count: 1,
            removed_waiter_count: 1,
        }
    );
    assert!(summary.removed_any());
    assert_eq!(locks.entry(held_resource).unwrap(), None);
    let waited_entry = locks.entry(waited_resource).unwrap().unwrap();
    assert_eq!(
        waited_entry.holders,
        vec![holder(other_tx, LockMode::Exclusive)]
    );
    assert!(waited_entry.waiters.is_empty());
}

#[test]
fn transaction_lock_coordinator_rejects_invalid_transaction_and_resource() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let valid_tx = transactions.begin().unwrap();
    let unknown_tx = TransactionId::new(99_999);
    let zero_tx = TransactionId::new(0);
    let valid_resource = table_resource(90);
    let invalid_resource = LockResource::Table {
        schema_id: 1,
        table_id: 0,
    };

    assert_transaction_error(transactions.acquire_lock(
        &locks,
        unknown_tx,
        valid_resource,
        LockMode::Shared,
    ));
    assert_transaction_error(transactions.acquire_lock(
        &locks,
        zero_tx,
        valid_resource,
        LockMode::Shared,
    ));
    assert_transaction_error(transactions.acquire_lock(
        &locks,
        valid_tx,
        invalid_resource,
        LockMode::Shared,
    ));
    assert_transaction_error(transactions.release_lock(&locks, valid_tx, invalid_resource));
    assert_transaction_error(transactions.release_all_locks(&locks, unknown_tx));
    assert_transaction_error(transactions.release_all_locks(&locks, valid_tx));
    assert_eq!(locks.entry_count().unwrap(), 0);
}
