use andromeda_tx::{TransactionManager, TransactionStatus};

#[test]
fn disposed_rolled_back_transaction_retains_status_history() {
    let manager = TransactionManager::new();
    let tx = manager.begin().unwrap();

    manager.request_rollback(tx).unwrap();
    manager.rollback_durable(tx, 77).unwrap();
    manager.dispose(tx).unwrap();

    assert_eq!(manager.live_count().unwrap(), 0);
    assert!(manager.snapshot(tx).unwrap().is_none());
    assert_eq!(
        manager.status(tx).unwrap(),
        Some(TransactionStatus::RolledBack),
        "dispose removes live state only; MVCC status history must retain the durable rollback"
    );
}
