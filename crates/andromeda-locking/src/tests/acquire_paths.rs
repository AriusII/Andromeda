use super::fixtures::{grant, holder, table_resource};
use super::*;

#[test]
fn acquire_grants_immediately_when_compatible() {
    let manager = LockManager::new();
    let resource = table_resource();

    grant(&manager, 1, resource, LockMode::Shared);
    grant(&manager, 2, resource, LockMode::Shared);

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(
        entry.holders,
        vec![holder(1, LockMode::Shared), holder(2, LockMode::Shared)]
    );
    assert!(entry.waiters.is_empty());
}

#[test]
fn acquire_waits_with_blockers_when_incompatible() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();

    assert_eq!(
        manager
            .acquire(TransactionId::new(1), resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    let status = manager
        .acquire(TransactionId::new(2), resource, LockMode::Shared)
        .unwrap();

    assert_eq!(
        status,
        LockAcquireStatus::Waiting {
            sequence: 1,
            blockers: vec![LockHolder {
                tx_id: TransactionId::new(1),
                mode: LockMode::Exclusive,
            }],
        }
    );

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 1);
    assert_eq!(entry.waiters.len(), 1);
    assert_eq!(entry.waiters[0].tx_id, TransactionId::new(2));
    assert_eq!(entry.waiters[0].sequence, 1);
}

#[test]
fn acquire_reports_same_transaction_already_held_without_duplicate_holder() {
    let manager = LockManager::new();
    let resource = LockResource::row(1, 2, 3).unwrap();
    let tx_id = TransactionId::new(7);

    assert_eq!(
        manager.acquire(tx_id, resource, LockMode::Shared).unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager.acquire(tx_id, resource, LockMode::Shared).unwrap(),
        LockAcquireStatus::AlreadyHeld {
            held_mode: LockMode::Shared,
        }
    );

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 1);
    assert!(entry.waiters.is_empty());
}

#[test]
fn acquire_upgrades_shared_to_exclusive_in_place_when_unblocked() {
    let manager = LockManager::new();
    let resource = LockResource::row(1, 2, 3).unwrap();
    let tx_id = TransactionId::new(7);

    assert_eq!(
        manager.acquire(tx_id, resource, LockMode::Shared).unwrap(),
        LockAcquireStatus::Granted
    );

    assert_eq!(
        manager
            .acquire(tx_id, resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Upgraded {
            previous_mode: LockMode::Shared,
            new_mode: LockMode::Exclusive,
        }
    );

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 1);
    assert_eq!(entry.holders[0].tx_id, tx_id);
    assert_eq!(entry.holders[0].mode, LockMode::Exclusive);
    assert!(entry.waiters.is_empty());
}

#[test]
fn acquire_waits_upgrade_when_other_holders_block_shared_to_exclusive() {
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

    assert_eq!(
        manager
            .acquire(upgrading_tx, resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::WaitingUpgrade {
            sequence: 1,
            held_mode: LockMode::Shared,
            requested_mode: LockMode::Exclusive,
            blockers: vec![LockHolder {
                tx_id: blocking_tx,
                mode: LockMode::Shared,
            }],
        }
    );

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 2);
    assert_eq!(entry.holders[0].mode, LockMode::Shared);
    assert_eq!(entry.waiters.len(), 1);
    assert_eq!(entry.waiters[0].tx_id, upgrading_tx);
    assert_eq!(entry.waiters[0].mode, LockMode::Exclusive);
    assert_eq!(entry.waiters[0].sequence, 1);
}

#[test]
fn acquire_upgrades_intent_shared_to_intent_exclusive_when_compatible() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();
    let upgrading_tx = TransactionId::new(10);
    let compatible_tx = TransactionId::new(11);

    assert_eq!(
        manager
            .acquire(upgrading_tx, resource, LockMode::IntentShared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(compatible_tx, resource, LockMode::IntentShared)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    assert_eq!(
        manager
            .acquire(upgrading_tx, resource, LockMode::IntentExclusive)
            .unwrap(),
        LockAcquireStatus::Upgraded {
            previous_mode: LockMode::IntentShared,
            new_mode: LockMode::IntentExclusive,
        }
    );

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 2);
    assert_eq!(
        entry
            .holders
            .iter()
            .find(|holder| holder.tx_id == upgrading_tx)
            .unwrap()
            .mode,
        LockMode::IntentExclusive
    );
    assert!(entry.waiters.is_empty());
}
