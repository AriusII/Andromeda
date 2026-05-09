//! Transaction/locking trace bridge contracts.
//!
//! Lock history traces owned entirely by the lock manager live in
//! `andromeda-locking`. This target keeps the release-all transaction trace
//! contracts that must pass through `TransactionManager`.

use andromeda_locking::{LockAcquireStatus, LockManager, LockMode, LockResource};
use andromeda_time::EngineTimestamp;
use andromeda_transaction::{LockReleaseAllTrace, TransactionManager, TransactionState};

#[test]
fn committed_transaction_release_all_trace_uses_coordinator_cleanup_summary() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let tx_id = transactions.begin().unwrap();
    let resource = LockResource::table(1, 10).unwrap();
    let ts = EngineTimestamp::from_unix_millis(4000);

    assert_eq!(
        transactions
            .acquire_lock(&locks, tx_id, resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    transactions.request_commit(tx_id).unwrap();
    transactions.commit_durable(tx_id, 1).unwrap();

    let cleanup_result = transactions
        .release_all_locks_with_evidence(&locks, tx_id)
        .unwrap();
    let trace = LockReleaseAllTrace::new(
        tx_id,
        cleanup_result.summary.affected_resource_count,
        TransactionState::Committed,
        ts,
    );

    assert_eq!(trace.tx_id, tx_id);
    assert_eq!(trace.resources_released, 1);
    assert_eq!(trace.terminal_state, TransactionState::Committed);
    assert_eq!(trace.timestamp, ts);
    assert!(locks.entry(resource).unwrap().is_none());
}

#[test]
fn rolled_back_transaction_release_all_trace_uses_coordinator_cleanup_summary() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let tx_id = transactions.begin().unwrap();
    let resource1 = LockResource::table(1, 10).unwrap();
    let resource2 = LockResource::table(1, 20).unwrap();
    let ts = EngineTimestamp::from_unix_millis(7000);

    assert_eq!(
        transactions
            .acquire_lock(&locks, tx_id, resource1, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        transactions
            .acquire_lock(&locks, tx_id, resource2, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    transactions.request_rollback(tx_id).unwrap();
    transactions.rollback_durable(tx_id, 2).unwrap();

    let cleanup_result = transactions
        .release_all_locks_with_evidence(&locks, tx_id)
        .unwrap();
    let trace = LockReleaseAllTrace::new(
        tx_id,
        cleanup_result.summary.affected_resource_count,
        TransactionState::RolledBack,
        ts,
    );

    assert_eq!(trace.tx_id, tx_id);
    assert_eq!(trace.resources_released, 2);
    assert_eq!(trace.terminal_state, TransactionState::RolledBack);
    assert_eq!(trace.timestamp, ts);
    assert_eq!(locks.entry_count().unwrap(), 0);
}
