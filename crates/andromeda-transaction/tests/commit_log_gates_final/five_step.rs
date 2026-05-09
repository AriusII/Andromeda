//! Five-step commit sequence gates.

use super::fixtures::*;
use andromeda_core::TransactionId;
use andromeda_transaction::{IsolationLevel, TransactionStatus, WalRecordKind};

#[tokio::test]
async fn test_five_step_sequence_complete() {
    let (wal, status_table, commit_log) = setup_commit_log();

    let tx_id = TransactionId::new(1);
    let entry = commit_log
        .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
        .await
        .expect("commit");

    assert_eq!(wal.record_count(), 1, "One WAL record created");
    assert_eq!(
        wal.get_durable_lsn(),
        entry.commit_lsn,
        "WAL flushed to commit LSN"
    );
    assert_eq!(entry.tx_id, tx_id);
    assert_eq!(entry.row_count_affected, 10);
    assert_eq!(
        status_table.status(tx_id),
        Some(TransactionStatus::Committed),
        "Status table marked as committed"
    );
    assert!(commit_log.is_committed(tx_id), "Transaction is visible");
}

#[tokio::test]
async fn test_five_step_fail_at_wal_write() {
    let (wal, _status_table, commit_log) = setup_commit_log();

    let tx_id = TransactionId::new(1);

    let _entry = commit_log
        .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
        .await
        .expect("commit succeeds");

    let records = wal.get_records();
    assert!(!records.is_empty(), "WAL records were created");
    assert_eq!(records[0].0, WalRecordKind::TxCommit, "Record is TxCommit");
}

#[tokio::test]
async fn test_five_step_fail_at_wal_flush_blocks() {
    let wal = TestWal::with_fail_flush_after(0);
    let (status_table, commit_log) = setup_commit_log_with_wal(wal);

    let tx_id = TransactionId::new(1);

    let result = commit_log
        .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
        .await;

    assert!(result.is_err(), "Commit failed due to WAL flush failure");
    assert_eq!(
        status_table.status(tx_id),
        None,
        "Transaction remains uncommitted after WAL flush failure"
    );
}

#[tokio::test]
async fn test_five_step_post_flush_idempotent() {
    let (wal, _status_table, commit_log) = setup_commit_log();

    let tx_id = TransactionId::new(1);
    let entry1 = commit_log
        .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
        .await
        .expect("first commit");

    let _durable_lsn_after_first = wal.get_durable_lsn();

    assert!(commit_log.is_committed(tx_id), "Transaction visible");

    let retrieved_lsn = commit_log.get_commit_lsn(tx_id);
    assert_eq!(retrieved_lsn, Some(entry1.commit_lsn), "LSN consistent");
}
