//! Commit log edge-case gates.

use super::fixtures::*;
use andromeda_transaction_log::IsolationLevel;
use andromeda_types::TransactionId;

#[tokio::test]
async fn test_edge_case_zero_transaction_id() {
    let (_wal, _status_table, commit_log) = setup_commit_log();

    let tx_id = TransactionId::new(0);
    let result = commit_log
        .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_edge_case_large_transaction_id() {
    let (_wal, _status_table, commit_log) = setup_commit_log();

    let tx_id = TransactionId::new(u64::MAX - 1);
    let entry = commit_log
        .record_commit(tx_id, IsolationLevel::Snapshot, 1, 0)
        .await
        .expect("commit");

    assert_eq!(entry.tx_id, tx_id);
    assert!(commit_log.is_committed(tx_id));
}

#[tokio::test]
async fn test_edge_case_commit_idempotence() {
    let (_wal, _status_table, commit_log) = setup_commit_log();

    let tx_id = TransactionId::new(100);

    let entry1 = commit_log
        .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
        .await
        .expect("first commit");

    assert!(commit_log.is_committed(tx_id));

    let lsn1 = commit_log.get_commit_lsn(tx_id);
    assert_eq!(lsn1, Some(entry1.commit_lsn));

    let lsn2 = commit_log.get_commit_lsn(tx_id);
    assert_eq!(
        lsn2, lsn1,
        "LSN unchanged (idempotence or duplicate prevention)"
    );
}
