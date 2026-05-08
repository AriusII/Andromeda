use super::*;

#[test]
fn acquire_does_not_bypass_older_incompatible_waiter() {
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
            .acquire(TransactionId::new(2), resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Waiting {
            sequence: 1,
            blockers: vec![LockHolder {
                tx_id: TransactionId::new(1),
                mode: LockMode::Shared,
            }],
        }
    );

    assert_eq!(
        manager
            .acquire(TransactionId::new(3), resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting {
            sequence: 2,
            blockers: vec![],
        }
    );

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 1);
    assert_eq!(entry.waiters.len(), 2);
    assert_eq!(entry.waiters[0].tx_id, TransactionId::new(2));
    assert_eq!(entry.waiters[0].sequence, 1);
    assert_eq!(entry.waiters[1].tx_id, TransactionId::new(3));
    assert_eq!(entry.waiters[1].sequence, 2);
}

#[test]
fn acquire_reentrant_waiting_upgrade_does_not_duplicate_waiter_or_holder() {
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

    let first = manager
        .acquire(upgrading_tx, resource, LockMode::Exclusive)
        .unwrap();
    let second = manager
        .acquire(upgrading_tx, resource, LockMode::Exclusive)
        .unwrap();

    assert!(matches!(
        first,
        LockAcquireStatus::WaitingUpgrade { sequence: 1, .. }
    ));
    assert!(matches!(
        second,
        LockAcquireStatus::WaitingUpgrade { sequence: 1, .. }
    ));

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 2);
    assert_eq!(
        entry
            .holders
            .iter()
            .filter(|holder| holder.tx_id == upgrading_tx)
            .count(),
        1
    );
    assert_eq!(entry.waiters.len(), 1);
    assert_eq!(entry.waiters[0].tx_id, upgrading_tx);
    assert_eq!(entry.waiters[0].sequence, 1);
}

#[test]
fn acquire_stronger_held_mode_reports_already_held_without_duplicate() {
    let manager = LockManager::new();
    let resource = LockResource::row(1, 2, 3).unwrap();
    let tx_id = TransactionId::new(7);

    assert_eq!(
        manager
            .acquire(tx_id, resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager.acquire(tx_id, resource, LockMode::Shared).unwrap(),
        LockAcquireStatus::AlreadyHeld {
            held_mode: LockMode::Exclusive,
        }
    );

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 1);
    assert_eq!(entry.holders[0].mode, LockMode::Exclusive);
    assert!(entry.waiters.is_empty());
}

#[test]
fn acquire_waiter_sequences_are_fifo_and_deterministic() {
    let manager = LockManager::new();
    let resource = LockResource::schema(1).unwrap();

    assert_eq!(
        manager
            .acquire(TransactionId::new(1), resource, LockMode::SchemaExclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    let first_wait = manager
        .acquire(TransactionId::new(2), resource, LockMode::SchemaShared)
        .unwrap();
    let second_wait = manager
        .acquire(TransactionId::new(3), resource, LockMode::Shared)
        .unwrap();

    assert!(matches!(
        first_wait,
        LockAcquireStatus::Waiting { sequence: 1, .. }
    ));
    assert!(matches!(
        second_wait,
        LockAcquireStatus::Waiting { sequence: 2, .. }
    ));

    let entry = manager.entry(resource).unwrap().unwrap();
    let waiters: Vec<(TransactionId, u64)> = entry
        .waiters
        .iter()
        .map(|waiter| (waiter.tx_id, waiter.sequence))
        .collect();
    assert_eq!(
        waiters,
        vec![(TransactionId::new(2), 1), (TransactionId::new(3), 2)]
    );
}
