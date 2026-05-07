use super::super::*;
use super::support::*;

use std::sync::Arc;

#[test]
fn test_commit_log_rebuilds_missing_status_from_local_records() {
    let (_wal, status_table, commit_log) = setup_commit_log();

    let committed = TransactionId::new(6);
    let rolled_back = TransactionId::new(7);
    commit_log
        .restore_commit_entry(durable_commit_entry(
            committed,
            Lsn::new(11),
            2,
            IsolationLevel::Snapshot,
        ))
        .unwrap();
    commit_log
        .restore_rollback_entry(durable_rollback_entry(rolled_back, Lsn::new(12), 3))
        .unwrap();

    assert_eq!(
        status_table.status(committed),
        Some(TransactionStatus::Committed)
    );
    assert_eq!(
        status_table.status(rolled_back),
        Some(TransactionStatus::RolledBack)
    );

    let status_table = Arc::new(TransactionStatusTable::new());
    let recovered = CommitLogManager::new(MockWal::new(), status_table.clone());
    recovered.seed_commit_entry_without_status(durable_commit_entry(
        committed,
        Lsn::new(21),
        2,
        IsolationLevel::Serializable,
    ));
    recovered.seed_rollback_entry_without_status(durable_rollback_entry(
        rolled_back,
        Lsn::new(22),
        3,
    ));

    let summary = recovered.rebuild_status_from_records().unwrap();
    assert_eq!(summary.committed_restored, 1);
    assert_eq!(summary.rolled_back_restored, 1);
    assert_eq!(
        status_table.status(committed),
        Some(TransactionStatus::Committed)
    );
    assert_eq!(
        status_table.status(rolled_back),
        Some(TransactionStatus::RolledBack)
    );
}

#[test]
fn test_commit_log_rebuild_rejects_seeded_terminal_entry_without_durable_coverage() {
    let (_wal, status_table, commit_log) = setup_commit_log();

    let committed = TransactionId::new(28);
    commit_log.seed_commit_entry_without_status(commit_entry_with_lsns(
        committed,
        Lsn::new(9),
        Lsn::new(8),
        1,
        IsolationLevel::Snapshot,
    ));

    assert!(!commit_log.is_committed(committed));
    assert_eq!(status_table.status(committed), None);
    assert_eq!(commit_log.get_commit_lsn(committed), None);

    let err = commit_log
        .rebuild_status_from_records()
        .expect_err("rebuild must not publish invalid terminal evidence");
    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert_eq!(status_table.status(committed), None);
}

#[test]
fn test_commit_log_replay_rejects_terminal_records_without_durable_coverage() {
    let (_wal, status_table, commit_log) = setup_commit_log();

    let committed = TransactionId::new(34);
    let commit_err = commit_log
        .replay_tx_wal_record(TxWalReplayRecord::Commit(commit_entry_with_lsns(
            committed,
            Lsn::new(9),
            Lsn::new(8),
            1,
            IsolationLevel::Snapshot,
        )))
        .expect_err("replay must reject commit records not covered by durable LSN");

    assert_eq!(commit_err.kind(), AndromedaErrorKind::Storage);
    assert_eq!(status_table.status(committed), None);
    assert!(!commit_log.is_committed(committed));
    assert_eq!(commit_log.get_commit_lsn(committed), None);

    let rolled_back = TransactionId::new(35);
    let rollback_err = commit_log
        .replay_tx_wal_record(TxWalReplayRecord::Rollback(rollback_entry_with_lsns(
            rolled_back,
            Lsn::new(10),
            Lsn::new(9),
            0xA11,
        )))
        .expect_err("replay must reject rollback records not covered by durable LSN");

    assert_eq!(rollback_err.kind(), AndromedaErrorKind::Storage);
    assert_eq!(status_table.status(rolled_back), None);
    assert!(!commit_log.is_rolled_back(rolled_back));
    assert_eq!(commit_log.get_rollback_lsn(rolled_back), None);
}

#[test]
fn test_commit_log_replay_advances_in_flight_status_with_durable_terminal_evidence()
-> AndromedaResult<()> {
    let (_wal, status_table, commit_log) = setup_commit_log();
    let tx_id = TransactionId::new(36);

    status_table.record_in_flight(tx_id)?;

    let action = commit_log.replay_tx_wal_record(TxWalReplayRecord::commit(
        tx_id,
        Lsn::new(11),
        EngineTimestamp::from_unix_millis(1),
        1,
        IsolationLevel::Snapshot,
    ))?;

    assert_eq!(action, TxWalReplayAction::CommitRestored);
    assert_eq!(
        status_table.status(tx_id),
        Some(TransactionStatus::Committed)
    );
    assert_eq!(commit_log.get_commit_lsn(tx_id), Some(Lsn::new(11)));
    Ok(())
}

#[test]
fn test_commit_log_replay_conflict_does_not_leave_partial_terminal_entry() -> AndromedaResult<()> {
    let (_wal, status_table, commit_log) = setup_commit_log();
    let tx_id = TransactionId::new(37);

    status_table.record_rolled_back_after_durable_wal(tx_id, Lsn::new(10), Lsn::new(10))?;

    let err = commit_log
        .replay_tx_wal_record(TxWalReplayRecord::commit(
            tx_id,
            Lsn::new(11),
            EngineTimestamp::from_unix_millis(1),
            1,
            IsolationLevel::Snapshot,
        ))
        .expect_err("conflicting terminal replay must be rejected before insert");

    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(
        status_table.status(tx_id),
        Some(TransactionStatus::RolledBack)
    );
    assert_eq!(commit_log.get_commit_lsn(tx_id), None);
    assert!(!commit_log.is_committed(tx_id));
    Ok(())
}
