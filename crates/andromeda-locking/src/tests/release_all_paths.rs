use super::*;

#[test]
fn release_all_releases_multiple_resources_and_keeps_unaffected_holders() {
    let manager = LockManager::new();
    let table = LockResource::table(1, 2).unwrap();
    let row = LockResource::row(1, 2, 3).unwrap();
    let ending_tx = TransactionId::new(21);
    let remaining_tx = TransactionId::new(22);

    assert_eq!(
        manager.acquire(ending_tx, table, LockMode::Shared).unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager.acquire(ending_tx, row, LockMode::Shared).unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(remaining_tx, table, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    let summary = manager.release_all(ending_tx).unwrap();

    assert_eq!(
        summary,
        LockReleaseAllSummary {
            affected_resource_count: 2,
            released_holder_count: 2,
            removed_waiter_count: 0,
        }
    );
    assert!(summary.removed_any());
    let table_entry = manager.entry(table).unwrap().unwrap();
    assert_eq!(
        table_entry.holders,
        vec![LockHolder {
            tx_id: remaining_tx,
            mode: LockMode::Shared,
        }]
    );
    assert!(table_entry.waiters.is_empty());
    assert_eq!(manager.entry(row).unwrap(), None);
    assert_eq!(manager.entry_count().unwrap(), 1);
}

#[test]
fn release_all_removes_waiters_for_ending_transaction() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();
    let holder_tx = TransactionId::new(31);
    let waiting_tx = TransactionId::new(32);

    assert_eq!(
        manager
            .acquire(holder_tx, resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert!(matches!(
        manager
            .acquire(waiting_tx, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { .. }
    ));

    let summary = manager.release_all(waiting_tx).unwrap();

    assert_eq!(
        summary,
        LockReleaseAllSummary {
            affected_resource_count: 1,
            released_holder_count: 0,
            removed_waiter_count: 1,
        }
    );
    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(
        entry.holders,
        vec![LockHolder {
            tx_id: holder_tx,
            mode: LockMode::Exclusive,
        }]
    );
    assert!(entry.waiters.is_empty());
}

#[test]
fn release_all_promotes_waiters_per_resource_after_holder_removal() {
    let manager = LockManager::new();
    let table = LockResource::table(1, 2).unwrap();
    let row = LockResource::row(1, 2, 3).unwrap();
    let ending_tx = TransactionId::new(41);
    let table_waiter = TransactionId::new(42);
    let row_waiter = TransactionId::new(43);

    assert_eq!(
        manager
            .acquire(ending_tx, table, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(ending_tx, row, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert!(matches!(
        manager
            .acquire(table_waiter, table, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { .. }
    ));
    assert!(matches!(
        manager.acquire(row_waiter, row, LockMode::Shared).unwrap(),
        LockAcquireStatus::Waiting { .. }
    ));

    let summary = manager.release_all(ending_tx).unwrap();

    assert_eq!(
        summary,
        LockReleaseAllSummary {
            affected_resource_count: 2,
            released_holder_count: 2,
            removed_waiter_count: 0,
        }
    );
    let table_entry = manager.entry(table).unwrap().unwrap();
    assert_eq!(
        table_entry.holders,
        vec![LockHolder {
            tx_id: table_waiter,
            mode: LockMode::Shared,
        }]
    );
    assert!(table_entry.waiters.is_empty());

    let row_entry = manager.entry(row).unwrap().unwrap();
    assert_eq!(
        row_entry.holders,
        vec![LockHolder {
            tx_id: row_waiter,
            mode: LockMode::Shared,
        }]
    );
    assert!(row_entry.waiters.is_empty());
}

#[test]
fn release_all_removes_empty_entries_after_terminal_cleanup() {
    let manager = LockManager::new();
    let schema = LockResource::schema(1).unwrap();
    let row = LockResource::row(1, 2, 3).unwrap();
    let ending_tx = TransactionId::new(51);

    assert_eq!(
        manager
            .acquire(ending_tx, schema, LockMode::SchemaShared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    manager
        .enqueue_waiter(row, ending_tx, LockMode::Exclusive)
        .unwrap();

    let summary = manager.release_all(ending_tx).unwrap();

    assert_eq!(
        summary,
        LockReleaseAllSummary {
            affected_resource_count: 2,
            released_holder_count: 1,
            removed_waiter_count: 1,
        }
    );
    assert_eq!(manager.entry(schema).unwrap(), None);
    assert_eq!(manager.entry(row).unwrap(), None);
    assert_eq!(manager.entry_count().unwrap(), 0);
}

#[test]
fn release_all_rejects_zero_transaction_id_without_mutation() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();

    assert_eq!(
        manager
            .acquire(TransactionId::new(61), resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    let err = manager.release_all(TransactionId::new(0)).unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(manager.entry_count().unwrap(), 1);
    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 1);
    assert!(entry.waiters.is_empty());
}

#[test]
fn release_all_leaves_unaffected_resource_entries_unchanged() {
    let manager = LockManager::new();
    let ending_resource = LockResource::table(1, 2).unwrap();
    let unaffected_resource = LockResource::row(1, 2, 3).unwrap();
    let ending_tx = TransactionId::new(71);
    let unaffected_tx = TransactionId::new(72);

    assert_eq!(
        manager
            .acquire(ending_tx, ending_resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(unaffected_tx, unaffected_resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    let summary = manager.release_all(ending_tx).unwrap();

    assert_eq!(
        summary,
        LockReleaseAllSummary {
            affected_resource_count: 1,
            released_holder_count: 1,
            removed_waiter_count: 0,
        }
    );
    assert_eq!(manager.entry(ending_resource).unwrap(), None);
    let unaffected_entry = manager.entry(unaffected_resource).unwrap().unwrap();
    assert_eq!(
        unaffected_entry.holders,
        vec![LockHolder {
            tx_id: unaffected_tx,
            mode: LockMode::Exclusive,
        }]
    );
    assert!(unaffected_entry.waiters.is_empty());
    assert_eq!(manager.entry_count().unwrap(), 1);
}
