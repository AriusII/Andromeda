use super::fixtures::{grant, holder, table_resource};
use super::*;

#[test]
fn release_removes_holder_and_keeps_non_empty_entry() {
    let manager = LockManager::new();
    let resource = table_resource();

    grant(&manager, 1, resource, LockMode::Shared);
    grant(&manager, 2, resource, LockMode::Shared);

    assert!(manager.release(TransactionId::new(1), resource).unwrap());

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders, vec![holder(2, LockMode::Shared)]);
    assert!(entry.waiters.is_empty());
    assert_eq!(manager.entry_count().unwrap(), 1);
}

#[test]
fn release_promotes_compatible_shared_waiters_in_fifo_order() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();

    assert_eq!(
        manager
            .acquire(TransactionId::new(1), resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert!(matches!(
        manager
            .acquire(TransactionId::new(2), resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { sequence: 1, .. }
    ));
    assert!(matches!(
        manager
            .acquire(TransactionId::new(3), resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { sequence: 2, .. }
    ));

    assert!(manager.release(TransactionId::new(1), resource).unwrap());

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(
        entry.holders,
        vec![
            LockHolder {
                tx_id: TransactionId::new(2),
                mode: LockMode::Shared,
            },
            LockHolder {
                tx_id: TransactionId::new(3),
                mode: LockMode::Shared,
            },
        ]
    );
    assert!(entry.waiters.is_empty());
}

#[test]
fn release_promotes_waiting_exclusive_when_compatible_after_holder_removal() {
    let manager = LockManager::new();
    let resource = LockResource::row(1, 2, 3).unwrap();

    assert_eq!(
        manager
            .acquire(TransactionId::new(1), resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert!(matches!(
        manager
            .acquire(TransactionId::new(2), resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Waiting { sequence: 1, .. }
    ));

    assert!(manager.release(TransactionId::new(1), resource).unwrap());

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(
        entry.holders,
        vec![LockHolder {
            tx_id: TransactionId::new(2),
            mode: LockMode::Exclusive,
        }]
    );
    assert!(entry.waiters.is_empty());
}

#[test]
fn release_preserves_fifo_when_incompatible_waiter_blocks_later_compatible_waiter() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();

    assert_eq!(
        manager
            .acquire(TransactionId::new(1), resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(TransactionId::new(2), resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert!(matches!(
        manager
            .acquire(TransactionId::new(3), resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Waiting { sequence: 1, .. }
    ));
    assert!(matches!(
        manager
            .acquire(TransactionId::new(4), resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { sequence: 2, .. }
    ));

    assert!(manager.release(TransactionId::new(1), resource).unwrap());

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(
        entry.holders,
        vec![LockHolder {
            tx_id: TransactionId::new(2),
            mode: LockMode::Shared,
        }]
    );
    let waiters: Vec<(TransactionId, LockMode, u64)> = entry
        .waiters
        .iter()
        .map(|waiter| (waiter.tx_id, waiter.mode, waiter.sequence))
        .collect();
    assert_eq!(
        waiters,
        vec![
            (TransactionId::new(3), LockMode::Exclusive, 1),
            (TransactionId::new(4), LockMode::Shared, 2),
        ]
    );
}

#[test]
fn release_promotes_waiting_upgrade_in_place() {
    let manager = LockManager::new();
    let resource = LockResource::row(1, 2, 3).unwrap();
    let upgrading_tx = TransactionId::new(7);
    let blocking_tx = TransactionId::new(8);

    assert_eq!(
        manager
            .acquire(upgrading_tx, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(blocking_tx, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert!(matches!(
        manager
            .acquire(upgrading_tx, resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::WaitingUpgrade { sequence: 1, .. }
    ));

    assert!(manager.release(blocking_tx, resource).unwrap());

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 1);
    assert_eq!(entry.holders[0].tx_id, upgrading_tx);
    assert_eq!(entry.holders[0].mode, LockMode::Exclusive);
    assert!(entry.waiters.is_empty());
}

#[test]
fn release_removes_empty_entry_after_final_holder_release() {
    let manager = LockManager::new();
    let resource = LockResource::schema(1).unwrap();

    assert_eq!(
        manager
            .acquire(TransactionId::new(1), resource, LockMode::SchemaShared)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    assert!(manager.release(TransactionId::new(1), resource).unwrap());
    assert_eq!(manager.entry(resource).unwrap(), None);
    assert_eq!(manager.entry_count().unwrap(), 0);
}

#[test]
fn release_promotion_does_not_create_duplicate_holder_for_waiting_upgrade() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();
    let upgrading_tx = TransactionId::new(10);
    let blocking_tx = TransactionId::new(11);

    assert_eq!(
        manager
            .acquire(upgrading_tx, resource, LockMode::IntentShared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(blocking_tx, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert!(matches!(
        manager
            .acquire(upgrading_tx, resource, LockMode::IntentExclusive)
            .unwrap(),
        LockAcquireStatus::WaitingUpgrade { sequence: 1, .. }
    ));

    assert!(manager.release(blocking_tx, resource).unwrap());

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(
        entry
            .holders
            .iter()
            .filter(|holder| holder.tx_id == upgrading_tx)
            .count(),
        1
    );
    assert_eq!(entry.holders[0].mode, LockMode::IntentExclusive);
    assert!(entry.waiters.is_empty());
}
