use super::fixtures::{
    assert_granted, assert_transaction_error, holder, row_resource, schema_resource, table_resource,
};
use super::*;

#[test]
fn release_all_is_terminal_cleanup_only_and_removes_holders_and_waiters() {
    let manager = LockManager::new();
    let held_resource = table_resource(30);
    let waited_resource = row_resource(30, 300);
    let ending_tx = TransactionId::new(20);
    let other_tx = TransactionId::new(21);

    assert_granted(&manager, ending_tx, held_resource, LockMode::Shared);
    assert_granted(&manager, other_tx, waited_resource, LockMode::Exclusive);
    assert!(matches!(
        manager
            .acquire(ending_tx, waited_resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { .. }
    ));

    let summary = manager.release_all(ending_tx).unwrap();

    assert_eq!(
        summary,
        LockReleaseAllSummary {
            affected_resource_count: 2,
            released_holder_count: 1,
            removed_waiter_count: 1,
        }
    );
    assert!(summary.removed_any());
    assert_eq!(manager.entry(held_resource).unwrap(), None);
    let waited_entry = manager.entry(waited_resource).unwrap().unwrap();
    assert_eq!(
        waited_entry.holders,
        vec![holder(other_tx, LockMode::Exclusive)]
    );
    assert!(waited_entry.waiters.is_empty());
}

#[test]
fn invalid_ids_return_transaction_errors_without_mutating_lock_table() {
    let manager = LockManager::new();
    let valid_resource = table_resource(40);
    let invalid_resource = LockResource::Table {
        schema_id: 1,
        table_id: 0,
    };
    let valid_tx = TransactionId::new(30);
    let zero_tx = TransactionId::new(0);

    assert_transaction_error(manager.acquire(zero_tx, valid_resource, LockMode::Shared));
    assert_transaction_error(manager.acquire(valid_tx, invalid_resource, LockMode::Shared));
    assert_transaction_error(manager.release(zero_tx, valid_resource));
    assert_transaction_error(manager.release(valid_tx, invalid_resource));
    assert_transaction_error(manager.release_all(zero_tx));
    assert_transaction_error(manager.enqueue_waiter(invalid_resource, valid_tx, LockMode::Shared));
    assert_transaction_error(manager.record_holder(invalid_resource, valid_tx, LockMode::Shared));
    assert_transaction_error(manager.entry(invalid_resource));
    assert_transaction_error(manager.ensure_entry(invalid_resource));
    assert_eq!(manager.entry_count().unwrap(), 0);
}

#[test]
fn snapshot_is_deterministic_and_returns_owned_clones() {
    let manager = LockManager::new();
    let row = row_resource(50, 500);
    let schema = schema_resource(1);
    let table = table_resource(50);

    assert_granted(&manager, TransactionId::new(3), row, LockMode::Shared);
    assert_granted(
        &manager,
        TransactionId::new(1),
        schema,
        LockMode::SchemaShared,
    );
    assert_granted(
        &manager,
        TransactionId::new(2),
        table,
        LockMode::IntentShared,
    );

    let before_release = manager.snapshot().unwrap();
    let second_snapshot = manager.snapshot().unwrap();
    assert_eq!(before_release, second_snapshot);
    assert_eq!(
        before_release
            .iter()
            .map(|(resource, _)| *resource)
            .collect::<Vec<_>>(),
        vec![schema, table, row]
    );

    assert!(manager.release(TransactionId::new(1), schema).unwrap());

    assert_eq!(
        before_release
            .iter()
            .find(|(resource, _)| *resource == schema)
            .unwrap()
            .1
            .holders,
        vec![holder(TransactionId::new(1), LockMode::SchemaShared)]
    );
    assert!(
        manager
            .snapshot()
            .unwrap()
            .iter()
            .all(|(resource, _)| *resource != schema)
    );
}

#[test]
fn invalid_input_paths_return_errors_instead_of_panicking() {
    let manager = LockManager::new();
    let invalid_resource = LockResource::Row {
        schema_id: 1,
        table_id: 1,
        row_id: 0,
    };
    let zero_tx = TransactionId::new(0);

    let result = std::panic::catch_unwind(|| {
        assert_transaction_error(LockResource::schema(0));
        assert_transaction_error(LockResource::table(1, 0));
        assert_transaction_error(LockResource::page(1, 1, 0));
        assert_transaction_error(LockResource::row(1, 1, 0));
        assert_transaction_error(manager.ensure_entry(invalid_resource));
        assert_transaction_error(manager.entry(invalid_resource));
        assert_transaction_error(manager.acquire(zero_tx, invalid_resource, LockMode::Exclusive));
        assert_transaction_error(manager.enqueue_waiter(
            invalid_resource,
            zero_tx,
            LockMode::Shared,
        ));
        assert_transaction_error(manager.record_holder(
            invalid_resource,
            zero_tx,
            LockMode::Shared,
        ));
        assert_transaction_error(manager.release(zero_tx, invalid_resource));
        assert_transaction_error(manager.release_all(zero_tx));
    });

    assert!(result.is_ok());
    assert_eq!(manager.entry_count().unwrap(), 0);
}
