use super::fixtures::{assert_granted, assert_transaction_error, holder, row_resource};
use super::*;

#[test]
fn strict_2pl_growing_phase_allows_multiple_acquires_before_cleanup() {
    let locks = LockManager::new();
    let tx = TransactionId::new(10);
    let res1 = row_resource(100, 1000);
    let res2 = row_resource(100, 1001);

    assert_granted(&locks, tx, res1, LockMode::Shared);
    assert_granted(&locks, tx, res2, LockMode::Exclusive);

    let snap = locks.snapshot().unwrap();
    assert_eq!(snap.len(), 2);
}

#[test]
fn strict_2pl_terminal_cleanup_releases_all_held_locks() {
    let locks = LockManager::new();
    let tx = TransactionId::new(11);
    let resource = row_resource(101, 1010);

    assert_granted(&locks, tx, resource, LockMode::Shared);

    let summary = locks.release_all(tx).unwrap();

    assert_eq!(
        summary,
        LockReleaseAllSummary {
            affected_resource_count: 1,
            released_holder_count: 1,
            removed_waiter_count: 0,
        }
    );
    assert_eq!(locks.snapshot().unwrap(), Vec::new());
}

#[test]
fn strict_2pl_terminal_cleanup_removes_waiters_owned_by_ending_transaction() {
    let locks = LockManager::new();
    let tx = TransactionId::new(12);
    let blocker = TransactionId::new(13);
    let res1 = row_resource(102, 1020);
    let res2 = row_resource(102, 1021);

    assert_granted(&locks, tx, res1, LockMode::Shared);
    assert_granted(&locks, blocker, res2, LockMode::Exclusive);
    assert!(matches!(
        locks.acquire(tx, res2, LockMode::Shared).unwrap(),
        LockAcquireStatus::Waiting { .. }
    ));

    let summary = locks.release_all(tx).unwrap();

    assert_eq!(
        summary,
        LockReleaseAllSummary {
            affected_resource_count: 2,
            released_holder_count: 1,
            removed_waiter_count: 1,
        }
    );
    assert_eq!(locks.entry(res1).unwrap(), None);
    let res2_entry = locks.entry(res2).unwrap().unwrap();
    assert_eq!(
        res2_entry.holders,
        vec![holder(blocker, LockMode::Exclusive)]
    );
    assert!(res2_entry.waiters.is_empty());
}

#[test]
fn strict_2pl_release_all_rejects_zero_transaction_without_mutating_table() {
    let locks = LockManager::new();
    let tx = TransactionId::new(14);
    let resource = row_resource(106, 1060);

    assert_granted(&locks, tx, resource, LockMode::Shared);

    assert_transaction_error(locks.release_all(TransactionId::new(0)));

    let entry = locks.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders, vec![holder(tx, LockMode::Shared)]);
}
