use super::super::*;
use super::support::*;

#[tokio::test]
async fn test_commit_log_records_entry() {
    let (_wal, _status_table, commit_log) = setup_commit_log();

    let tx_id = TransactionId::new(1);
    let entry = commit_log
        .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
        .await
        .unwrap();

    assert!(commit_log.is_committed(tx_id));
    assert_eq!(commit_log.get_commit_lsn(tx_id), Some(entry.commit_lsn));
    assert_eq!(
        commit_log.get_commit_durable_lsn(tx_id),
        Some(entry.durable_lsn)
    );
    assert_eq!(commit_log.get_affected_rows(tx_id), Some(10));
}

#[tokio::test]
async fn test_commit_log_commit_is_idempotent_without_duplicate_wal_record() {
    let (wal, _status_table, commit_log) = setup_commit_log();

    let tx_id = TransactionId::new(1);
    let first = commit_log
        .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
        .await
        .unwrap();
    let second = commit_log
        .record_commit(tx_id, IsolationLevel::Snapshot, 99, 42)
        .await
        .unwrap();

    assert_eq!(first, second);
    assert_eq!(wal.records.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn test_commit_log_records_durable_rollback() {
    let (wal, status_table, commit_log) = setup_commit_log();

    let tx_id = TransactionId::new(2);
    let entry = commit_log.record_rollback(tx_id, 0xCAFE).await.unwrap();

    assert_eq!(entry.tx_id, tx_id);
    assert_eq!(entry.rollback_lsn, Lsn::new(1));
    assert_eq!(entry.durable_lsn, Lsn::new(1));
    assert_eq!(entry.parameter_hash, 0xCAFE);
    assert!(commit_log.is_rolled_back(tx_id));
    assert_eq!(
        commit_log.get_rollback_durable_lsn(tx_id),
        Some(entry.durable_lsn)
    );
    assert_eq!(
        status_table.status(tx_id),
        Some(TransactionStatus::RolledBack)
    );
    assert_eq!(*wal.durable_lsn.lock().unwrap(), entry.rollback_lsn);
}

#[tokio::test]
async fn test_commit_log_rollback_is_idempotent_without_duplicate_wal_record() {
    let (wal, _status_table, commit_log) = setup_commit_log();

    let tx_id = TransactionId::new(3);
    let first = commit_log.record_rollback(tx_id, 1).await.unwrap();
    let second = commit_log.record_rollback(tx_id, 2).await.unwrap();

    assert_eq!(first, second);
    assert_eq!(wal.records.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn test_commit_log_rejects_zero_tx_id() {
    let (_wal, _status_table, commit_log) = setup_commit_log();

    let result = commit_log
        .record_commit(TransactionId::new(0), IsolationLevel::Snapshot, 5, 0)
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), AndromedaErrorKind::Transaction);
}
