use super::*;

#[test]
fn acquire_rejects_invalid_transaction_id() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();

    let zero_tx = manager.acquire(TransactionId::new(0), resource, LockMode::Shared);
    assert_eq!(zero_tx.unwrap_err().kind(), AndromedaErrorKind::Transaction);
    assert_eq!(manager.entry_count().unwrap(), 0);
}

#[test]
fn acquire_rejects_invalid_resource_id() {
    let manager = LockManager::new();
    let resource = LockResource::Table {
        schema_id: 1,
        table_id: 0,
    };

    let invalid_resource = manager.acquire(TransactionId::new(1), resource, LockMode::Shared);
    assert_eq!(
        invalid_resource.unwrap_err().kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(manager.entry_count().unwrap(), 0);
}

#[test]
fn acquire_does_not_panic_on_grant_wait_or_reentrant_paths() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();

    let result = std::panic::catch_unwind(|| {
        manager
            .acquire(TransactionId::new(1), resource, LockMode::Shared)
            .unwrap();
        manager
            .acquire(TransactionId::new(1), resource, LockMode::Shared)
            .unwrap();
        manager
            .acquire(TransactionId::new(2), resource, LockMode::Exclusive)
            .unwrap();
    });

    assert!(result.is_ok());
}
