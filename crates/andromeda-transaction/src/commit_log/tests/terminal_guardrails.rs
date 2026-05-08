use super::super::*;
use super::support::*;

#[tokio::test]
async fn test_commit_log_rejects_short_commit_flush_before_visibility() {
    let (_wal, status_table, commit_log) =
        setup_commit_log_with_wal(MockWal::short_flush_with_existing_record(Lsn::new(1)));

    let tx_id = TransactionId::new(30);
    let err = commit_log
        .record_commit(tx_id, IsolationLevel::Snapshot, 1, 0)
        .await
        .expect_err("short commit flush must fail");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert_commit_not_published(&commit_log, &status_table, tx_id);
}

#[tokio::test]
async fn test_commit_log_rejects_short_rollback_flush_before_status_update() {
    let (_wal, status_table, commit_log) =
        setup_commit_log_with_wal(MockWal::short_flush_with_existing_record(Lsn::new(1)));

    let tx_id = TransactionId::new(31);
    let err = commit_log
        .record_rollback(tx_id, 0)
        .await
        .expect_err("short rollback flush must fail");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert_rollback_not_published(&commit_log, &status_table, tx_id);
}

#[tokio::test]
async fn test_commit_log_rejects_conflicting_terminal_outcomes() {
    let (_wal, _status_table, commit_log) = setup_commit_log();

    let committed = TransactionId::new(4);
    commit_log
        .record_commit(committed, IsolationLevel::Snapshot, 1, 0)
        .await
        .unwrap();
    assert_eq!(
        commit_log
            .record_rollback(committed, 0)
            .await
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Transaction
    );

    let rolled_back = TransactionId::new(5);
    commit_log.record_rollback(rolled_back, 0).await.unwrap();
    assert_eq!(
        commit_log
            .record_commit(rolled_back, IsolationLevel::Snapshot, 1, 0)
            .await
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Transaction
    );
}

#[test]
fn test_commit_log_does_not_trust_forged_terminal_status_without_wal_entry() {
    let (_wal, status_table, commit_log) = setup_commit_log();

    let committed = TransactionId::new(26);
    let commit_err = status_table
        .record(committed, TransactionStatus::Committed)
        .expect_err("public status table must reject terminal commit without WAL evidence");

    assert_eq!(commit_err.kind(), AndromedaErrorKind::Transaction);
    assert_commit_not_published(&commit_log, &status_table, committed);

    let rolled_back = TransactionId::new(27);
    let rollback_err = status_table
        .record(rolled_back, TransactionStatus::RolledBack)
        .expect_err("public status table must reject terminal rollback without WAL evidence");

    assert_eq!(rollback_err.kind(), AndromedaErrorKind::Transaction);
    assert_rollback_not_published(&commit_log, &status_table, rolled_back);
}

#[test]
fn test_commit_log_restore_rejects_inconsistent_durability_evidence() {
    let (_wal, _status_table, commit_log) = setup_commit_log();

    let err = commit_log
        .restore_commit_entry(commit_entry_with_lsns(
            TransactionId::new(32),
            Lsn::new(5),
            Lsn::new(4),
            1,
            IsolationLevel::Snapshot,
        ))
        .expect_err("restore must reject commit entries not covered by durable LSN");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(!commit_log.is_committed(TransactionId::new(32)));
}

#[test]
fn test_commit_log_restore_rejects_rollback_without_durable_coverage() {
    let (_wal, status_table, commit_log) = setup_commit_log();
    let tx_id = TransactionId::new(33);

    let err = commit_log
        .restore_rollback_entry(rollback_entry_with_lsns(
            tx_id,
            Lsn::new(8),
            Lsn::new(7),
            0xD00D,
        ))
        .expect_err("restore must reject rollback entries not covered by durable LSN");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert_eq!(status_table.status(tx_id), None);
    assert!(!commit_log.is_rolled_back(tx_id));
    assert_eq!(commit_log.get_rollback_lsn(tx_id), None);
}
