//! TransactionManager savepoint gates.
//!
//! Pure savepoint stack and write-set behavior belongs to `andromeda-savepoint`.
//! This suite keeps only transaction-owner gates where savepoint behavior must
//! be coordinated through `TransactionManager`.

use andromeda_error::AndromedaErrorKind;
use andromeda_mvcc::TransactionStatus;
use andromeda_transaction::{TransactionManager, TransactionState};
use andromeda_types::TransactionId;

#[test]
fn create_savepoint_requires_active_in_flight_transaction() {
    let manager = TransactionManager::new();
    let tx = manager.begin().unwrap();

    let savepoint = manager.create_savepoint(tx, "s1").unwrap();
    assert_eq!(savepoint.id.get(), 1);

    manager.request_commit(tx).unwrap();
    let error = manager.create_savepoint(tx, "s2").unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
}

#[test]
fn create_savepoint_rejects_unknown_and_zero_transaction_ids() {
    let manager = TransactionManager::new();

    let unknown = TransactionId::new(999);
    let unknown_error = manager.create_savepoint(unknown, "sp").unwrap_err();
    assert_eq!(unknown_error.kind(), AndromedaErrorKind::Transaction);

    let zero = TransactionId::new(0);
    assert_eq!(
        manager.create_savepoint(zero, "sp").unwrap_err().kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(
        manager
            .rollback_to_savepoint(zero, "sp")
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(
        manager.release_savepoint(zero, "sp").unwrap_err().kind(),
        AndromedaErrorKind::Transaction
    );
}

#[test]
fn savepoint_ops_are_rejected_after_failed_or_poisoned_states() {
    let manager = TransactionManager::new();

    let failed = manager.begin().unwrap();
    manager.create_savepoint(failed, "before_fail").unwrap();
    manager.fail(failed).unwrap();

    let snapshot = manager.snapshot(failed).unwrap().unwrap();
    assert_eq!(snapshot.state_machine.state(), TransactionState::Failed);
    assert!(manager.create_savepoint(failed, "after_fail").is_err());
    assert!(
        manager
            .rollback_to_savepoint(failed, "before_fail")
            .is_err()
    );
    assert!(manager.release_savepoint(failed, "before_fail").is_err());

    let poisoned = manager.begin().unwrap();
    manager.create_savepoint(poisoned, "before_poison").unwrap();
    manager.poison(poisoned).unwrap();

    assert!(manager.create_savepoint(poisoned, "after_poison").is_err());
    assert!(
        manager
            .rollback_to_savepoint(poisoned, "before_poison")
            .is_err()
    );
    assert!(
        manager
            .release_savepoint(poisoned, "before_poison")
            .is_err()
    );
}

#[test]
fn savepoint_ops_are_rejected_while_transaction_is_terminalizing() {
    let manager = TransactionManager::new();

    let committing = manager.begin().unwrap();
    manager.create_savepoint(committing, "sp").unwrap();
    manager.request_commit(committing).unwrap();
    assert_eq!(
        manager
            .rollback_to_savepoint(committing, "sp")
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Transaction
    );

    let rolling_back = manager.begin().unwrap();
    manager.create_savepoint(rolling_back, "sp").unwrap();
    manager.request_rollback(rolling_back).unwrap();
    assert_eq!(
        manager
            .release_savepoint(rolling_back, "sp")
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Transaction
    );
}

#[test]
fn commit_and_rollback_requests_clear_savepoint_stack() {
    let manager = TransactionManager::new();

    let commit_tx = manager.begin().unwrap();
    manager.create_savepoint(commit_tx, "s1").unwrap();
    manager.create_savepoint(commit_tx, "s2").unwrap();
    assert_eq!(manager.savepoint_depth(commit_tx).unwrap(), 2);

    manager.request_commit(commit_tx).unwrap();
    assert_eq!(manager.savepoint_depth(commit_tx).unwrap(), 0);

    let rollback_tx = manager.begin().unwrap();
    manager.create_savepoint(rollback_tx, "s1").unwrap();
    assert_eq!(manager.savepoint_depth(rollback_tx).unwrap(), 1);

    manager.request_rollback(rollback_tx).unwrap();
    assert_eq!(manager.savepoint_depth(rollback_tx).unwrap(), 0);
}

#[test]
fn committed_transaction_keeps_status_after_dispose_and_has_no_live_savepoints() {
    let manager = TransactionManager::new();
    let tx = manager.begin().unwrap();

    manager.create_savepoint(tx, "before_commit").unwrap();
    manager.request_commit(tx).unwrap();
    manager.commit_durable(tx, 42).unwrap();
    assert_eq!(manager.savepoint_depth(tx).unwrap(), 0);

    manager.dispose(tx).unwrap();

    assert_eq!(manager.live_count().unwrap(), 0);
    assert!(manager.snapshot(tx).unwrap().is_none());
    assert_eq!(
        manager.status(tx).unwrap(),
        Some(TransactionStatus::Committed)
    );
}

#[test]
fn create_savepoint_does_not_advance_lsn_or_publish_status() {
    let manager = TransactionManager::new();
    let tx = manager.begin().unwrap();

    let before = manager.snapshot(tx).unwrap().unwrap();
    manager.create_savepoint(tx, "no_lsn_change").unwrap();
    let after = manager.snapshot(tx).unwrap().unwrap();

    assert_eq!(before.state_machine.state(), after.state_machine.state());
    assert_eq!(
        before.state_machine.durable_commit_lsn(),
        after.state_machine.durable_commit_lsn()
    );
    assert_eq!(before.status, after.status);
}

#[test]
fn savepoint_stacks_are_isolated_per_transaction() {
    let manager = TransactionManager::new();
    let tx_a = manager.begin().unwrap();
    let tx_b = manager.begin().unwrap();

    let sp_a = manager.create_savepoint(tx_a, "checkpoint").unwrap();
    let sp_b = manager.create_savepoint(tx_b, "checkpoint").unwrap();
    manager.create_savepoint(tx_a, "work").unwrap();

    assert_eq!(sp_a.id.get(), 1);
    assert_eq!(sp_b.id.get(), 1);
    assert_eq!(manager.savepoint_depth(tx_a).unwrap(), 2);
    assert_eq!(manager.savepoint_depth(tx_b).unwrap(), 1);

    let error = manager.rollback_to_savepoint(tx_b, "work").unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);

    manager.rollback_to_savepoint(tx_a, "checkpoint").unwrap();
    assert_eq!(manager.savepoint_depth(tx_a).unwrap(), 1);
    assert_eq!(manager.savepoint_depth(tx_b).unwrap(), 1);
}

#[test]
fn disposing_one_transaction_does_not_clear_another_transactions_savepoints() {
    let manager = TransactionManager::new();
    let tx_a = manager.begin().unwrap();
    let tx_b = manager.begin().unwrap();

    manager.create_savepoint(tx_b, "b_checkpoint").unwrap();

    manager.request_commit(tx_a).unwrap();
    manager.commit_durable(tx_a, 1).unwrap();
    manager.dispose(tx_a).unwrap();

    assert_eq!(manager.savepoint_depth(tx_b).unwrap(), 1);
}

#[test]
fn manager_snapshot_reports_savepoint_depth_after_rollback_and_release() {
    let manager = TransactionManager::new();
    let rollback_tx = manager.begin().unwrap();

    manager.create_savepoint(rollback_tx, "s0").unwrap();
    manager.create_savepoint(rollback_tx, "s1").unwrap();
    manager.create_savepoint(rollback_tx, "s2").unwrap();
    manager.rollback_to_savepoint(rollback_tx, "s0").unwrap();

    let rollback_record = manager.snapshot(rollback_tx).unwrap().unwrap();
    assert_eq!(rollback_record.savepoint_depth, 1);

    let release_tx = manager.begin().unwrap();
    manager.create_savepoint(release_tx, "alpha").unwrap();
    manager.create_savepoint(release_tx, "beta").unwrap();
    manager.release_savepoint(release_tx, "alpha").unwrap();

    let release_record = manager.snapshot(release_tx).unwrap().unwrap();
    assert_eq!(release_record.savepoint_depth, 0);
}

#[test]
fn savepoint_depth_on_unknown_transaction_is_error() {
    let manager = TransactionManager::new();
    let error = manager
        .savepoint_depth(TransactionId::new(777))
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
}

#[test]
fn create_savepoint_does_not_release_existing_locks() {
    use andromeda_locking::{LockAcquireStatus, LockManager, LockMode, LockResource};

    let manager = TransactionManager::new();
    let locks = LockManager::new();
    let tx = manager.begin().unwrap();

    let resource = LockResource::row(1, 1, 1).expect("valid row lock resource");
    manager
        .acquire_lock(&locks, tx, resource, LockMode::Exclusive)
        .unwrap();

    manager.create_savepoint(tx, "mid_lock").unwrap();

    let contender = manager.begin().unwrap();
    let status = manager
        .acquire_lock(&locks, contender, resource, LockMode::Exclusive)
        .unwrap();

    assert_ne!(status, LockAcquireStatus::Granted);
}
