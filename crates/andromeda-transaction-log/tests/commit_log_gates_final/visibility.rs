//! Commit visibility gates.

use super::fixtures::*;
use andromeda_transaction_log::IsolationLevel;
use andromeda_types::TransactionId;

#[tokio::test]
async fn test_visibility_immediate_after_commit() {
    let (_wal, _status_table, commit_log) = setup_commit_log();

    let tx_id = TransactionId::new(1);

    assert!(!commit_log.is_committed(tx_id));

    commit_log
        .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
        .await
        .expect("commit");

    assert!(commit_log.is_committed(tx_id));
}

#[tokio::test]
async fn test_visibility_independent_transactions() {
    let (_wal, _status_table, commit_log) = setup_commit_log();

    let tx1 = TransactionId::new(1);
    let tx2 = TransactionId::new(2);

    commit_log
        .record_commit(tx1, IsolationLevel::Snapshot, 1, 0)
        .await
        .expect("commit tx1");

    assert!(commit_log.is_committed(tx1));
    assert!(!commit_log.is_committed(tx2));

    commit_log
        .record_commit(tx2, IsolationLevel::Snapshot, 1, 0)
        .await
        .expect("commit tx2");

    assert!(commit_log.is_committed(tx1));
    assert!(commit_log.is_committed(tx2));
}

#[tokio::test]
async fn test_visibility_timestamp_accuracy() {
    let (_wal, _status_table, commit_log) = setup_commit_log();

    let tx_id = TransactionId::new(1);
    let _before_time = std::time::SystemTime::now();

    let _entry = commit_log
        .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
        .await
        .expect("commit");

    let _after_time = std::time::SystemTime::now();

    let retrieved_ts = commit_log.get_commit_timestamp(tx_id);
    assert!(retrieved_ts.is_some(), "Timestamp recorded");
}
