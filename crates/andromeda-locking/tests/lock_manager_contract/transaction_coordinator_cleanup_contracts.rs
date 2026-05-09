use super::fixtures::{
    assert_granted, assert_transaction_error, holder, row_resource, schema_resource, table_resource,
};
use super::*;

#[test]
fn terminal_cleanup_release_all_removes_owner_holders_and_waiters() {
    let locks = LockManager::new();
    let ending_tx = TransactionId::new(30);
    let other_tx = TransactionId::new(31);
    let held_resource = schema_resource(1);
    let waited_resource = row_resource(80, 800);

    assert_granted(&locks, ending_tx, held_resource, LockMode::SchemaShared);
    assert_granted(&locks, other_tx, waited_resource, LockMode::Exclusive);
    assert!(matches!(
        locks
            .acquire(ending_tx, waited_resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { .. }
    ));

    let summary = locks.release_all(ending_tx).unwrap();

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
fn terminal_cleanup_rejects_invalid_owner_or_resource_without_mutation() {
    let locks = LockManager::new();
    let valid_tx = TransactionId::new(40);
    let zero_tx = TransactionId::new(0);
    let valid_resource = table_resource(90);
    let invalid_resource = LockResource::Table {
        schema_id: 1,
        table_id: 0,
    };

    assert_transaction_error(locks.acquire(zero_tx, valid_resource, LockMode::Shared));
    assert_transaction_error(locks.acquire(valid_tx, invalid_resource, LockMode::Shared));
    assert_transaction_error(locks.release(valid_tx, invalid_resource));
    assert_transaction_error(locks.release_all(zero_tx));
    assert_eq!(locks.entry_count().unwrap(), 0);
}
