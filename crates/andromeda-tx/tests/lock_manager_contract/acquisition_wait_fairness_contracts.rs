use super::fixtures::{assert_granted, holder, row_resource, table_resource};
use super::*;

#[test]
fn shared_acquisition_is_compatible_and_reentrant_without_duplicate_holder() {
    let manager = LockManager::new();
    let resource = table_resource(10);
    let first_tx = TransactionId::new(1);
    let second_tx = TransactionId::new(2);

    assert_granted(&manager, first_tx, resource, LockMode::Shared);
    assert_granted(&manager, second_tx, resource, LockMode::Shared);
    assert_eq!(
        manager
            .acquire(first_tx, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::AlreadyHeld {
            held_mode: LockMode::Shared
        }
    );

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(
        entry.holders,
        vec![
            holder(first_tx, LockMode::Shared),
            holder(second_tx, LockMode::Shared),
        ]
    );
    assert!(entry.waiters.is_empty());
}

#[test]
fn exclusive_request_waits_with_deterministic_blocker_evidence() {
    let manager = LockManager::new();
    let resource = row_resource(10, 100);
    let reader = TransactionId::new(1);
    let writer = TransactionId::new(2);

    assert_granted(&manager, reader, resource, LockMode::Shared);

    assert_eq!(
        manager
            .acquire(writer, resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Waiting {
            sequence: 1,
            blockers: vec![holder(reader, LockMode::Shared)],
        }
    );

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 1);
    assert_eq!(entry.waiters.len(), 1);
    assert_eq!(entry.waiters[0].tx_id, writer);
    assert_eq!(entry.waiters[0].mode, LockMode::Exclusive);
    assert_eq!(entry.waiters[0].sequence, 1);
}

#[test]
fn release_promotes_fifo_compatible_waiters_without_bypassing_incompatible_front() {
    let manager = LockManager::new();
    let resource = table_resource(20);
    let first_reader = TransactionId::new(1);
    let second_reader = TransactionId::new(2);
    let waiting_writer = TransactionId::new(3);
    let later_reader = TransactionId::new(4);

    assert_granted(&manager, first_reader, resource, LockMode::Shared);
    assert_granted(&manager, second_reader, resource, LockMode::Shared);
    assert!(matches!(
        manager
            .acquire(waiting_writer, resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Waiting { sequence: 1, .. }
    ));
    assert!(matches!(
        manager
            .acquire(later_reader, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { sequence: 2, .. }
    ));

    assert!(manager.release(first_reader, resource).unwrap());
    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders, vec![holder(second_reader, LockMode::Shared)]);
    assert_eq!(entry.waiters[0].tx_id, waiting_writer);
    assert_eq!(entry.waiters[0].sequence, 1);
    assert_eq!(entry.waiters[1].tx_id, later_reader);
    assert_eq!(entry.waiters[1].sequence, 2);

    assert!(manager.release(second_reader, resource).unwrap());
    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(
        entry.holders,
        vec![holder(waiting_writer, LockMode::Exclusive)]
    );
    assert_eq!(entry.waiters.len(), 1);
    assert_eq!(entry.waiters[0].tx_id, later_reader);
}

#[test]
fn blocked_upgrade_is_promoted_in_place_after_blocker_release() {
    let manager = LockManager::new();
    let resource = row_resource(20, 200);
    let upgrading_tx = TransactionId::new(10);
    let blocking_tx = TransactionId::new(11);

    assert_granted(&manager, upgrading_tx, resource, LockMode::Shared);
    assert_granted(&manager, blocking_tx, resource, LockMode::Shared);
    assert_eq!(
        manager
            .acquire(upgrading_tx, resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::WaitingUpgrade {
            sequence: 1,
            held_mode: LockMode::Shared,
            requested_mode: LockMode::Exclusive,
            blockers: vec![holder(blocking_tx, LockMode::Shared)],
        }
    );

    assert!(manager.release(blocking_tx, resource).unwrap());

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(
        entry.holders,
        vec![holder(upgrading_tx, LockMode::Exclusive)]
    );
    assert!(entry.waiters.is_empty());
}
