use super::*;

use std::sync::Arc;

/// Mock WAL for testing commit log without async I/O
struct MockWal {
    records: std::sync::Mutex<Vec<Lsn>>,
    durable_lsn: std::sync::Mutex<Lsn>,
    short_flush_lsn: Option<Lsn>,
}

impl MockWal {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            records: std::sync::Mutex::new(Vec::new()),
            durable_lsn: std::sync::Mutex::new(Lsn::new(0)),
            short_flush_lsn: None,
        })
    }

    fn short_flush(short_flush_lsn: Lsn) -> Arc<Self> {
        Arc::new(Self {
            records: std::sync::Mutex::new(Vec::new()),
            durable_lsn: std::sync::Mutex::new(Lsn::new(0)),
            short_flush_lsn: Some(short_flush_lsn),
        })
    }
}

#[async_trait::async_trait]
impl InvocationWal for MockWal {
    async fn append(
        &self,
        _kind: WalRecordKind,
        _transaction_id: Option<TransactionId>,
        _payload: &[u8],
    ) -> AndromedaResult<Lsn> {
        let mut records = self.records.lock().unwrap();
        let next_lsn = Lsn::new(records.len() as u64 + 1);
        records.push(next_lsn);
        Ok(next_lsn)
    }

    async fn flush_through(&self, lsn: Lsn) -> AndromedaResult<Lsn> {
        let mut durable = self.durable_lsn.lock().unwrap();
        let durable_lsn = self.short_flush_lsn.unwrap_or(lsn);
        *durable = durable_lsn;
        Ok(durable_lsn)
    }
}

#[tokio::test]
async fn test_commit_log_records_entry() {
    let wal = MockWal::new();
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal, status_table);

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
    let wal = MockWal::new();
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal.clone(), status_table);

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
    let wal = MockWal::new();
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal.clone(), status_table.clone());

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
async fn test_commit_log_rejects_short_commit_flush_before_visibility() {
    let wal = MockWal::short_flush(Lsn::new(1));
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal.clone(), status_table.clone());

    wal.records.lock().unwrap().push(Lsn::new(1));
    let tx_id = TransactionId::new(30);
    let err = commit_log
        .record_commit(tx_id, IsolationLevel::Snapshot, 1, 0)
        .await
        .expect_err("short commit flush must fail");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert_eq!(status_table.status(tx_id), None);
    assert!(!commit_log.is_committed(tx_id));
    assert_eq!(commit_log.get_commit_lsn(tx_id), None);
}

#[tokio::test]
async fn test_commit_log_rejects_short_rollback_flush_before_status_update() {
    let wal = MockWal::short_flush(Lsn::new(1));
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal.clone(), status_table.clone());

    wal.records.lock().unwrap().push(Lsn::new(1));
    let tx_id = TransactionId::new(31);
    let err = commit_log
        .record_rollback(tx_id, 0)
        .await
        .expect_err("short rollback flush must fail");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert_eq!(status_table.status(tx_id), None);
    assert!(!commit_log.is_rolled_back(tx_id));
    assert_eq!(commit_log.get_rollback_lsn(tx_id), None);
}

#[tokio::test]
async fn test_commit_log_rollback_is_idempotent_without_duplicate_wal_record() {
    let wal = MockWal::new();
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal.clone(), status_table);

    let tx_id = TransactionId::new(3);
    let first = commit_log.record_rollback(tx_id, 1).await.unwrap();
    let second = commit_log.record_rollback(tx_id, 2).await.unwrap();

    assert_eq!(first, second);
    assert_eq!(wal.records.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn test_commit_log_rejects_conflicting_terminal_outcomes() {
    let wal = MockWal::new();
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal, status_table);

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
fn test_commit_log_rebuilds_missing_status_from_local_records() {
    let wal = MockWal::new();
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal, status_table.clone());

    let committed = TransactionId::new(6);
    let rolled_back = TransactionId::new(7);
    commit_log
        .restore_commit_entry(CommitLogEntry {
            tx_id: committed,
            commit_lsn: Lsn::new(11),
            durable_lsn: Lsn::new(11),
            timestamp: EngineTimestamp::from_unix_millis(1),
            row_count_affected: 2,
            isolation_level: IsolationLevel::Snapshot,
        })
        .unwrap();
    commit_log
        .restore_rollback_entry(RollbackLogEntry {
            tx_id: rolled_back,
            rollback_lsn: Lsn::new(12),
            durable_lsn: Lsn::new(12),
            timestamp: EngineTimestamp::from_unix_millis(2),
            parameter_hash: 3,
        })
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
    recovered.seed_commit_entry_without_status(CommitLogEntry {
        tx_id: committed,
        commit_lsn: Lsn::new(21),
        durable_lsn: Lsn::new(21),
        timestamp: EngineTimestamp::from_unix_millis(1),
        row_count_affected: 2,
        isolation_level: IsolationLevel::Serializable,
    });
    recovered.seed_rollback_entry_without_status(RollbackLogEntry {
        tx_id: rolled_back,
        rollback_lsn: Lsn::new(22),
        durable_lsn: Lsn::new(22),
        timestamp: EngineTimestamp::from_unix_millis(2),
        parameter_hash: 3,
    });

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
fn test_commit_log_does_not_trust_forged_terminal_status_without_wal_entry() {
    let wal = MockWal::new();
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal, status_table.clone());

    let committed = TransactionId::new(26);
    let commit_err = status_table
        .record(committed, TransactionStatus::Committed)
        .expect_err("public status table must reject terminal commit without WAL evidence");

    assert_eq!(commit_err.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(status_table.status(committed), None);
    assert!(!commit_log.is_committed(committed));
    assert_eq!(commit_log.get_commit_lsn(committed), None);
    assert_eq!(commit_log.get_commit_durable_lsn(committed), None);

    let rolled_back = TransactionId::new(27);
    let rollback_err = status_table
        .record(rolled_back, TransactionStatus::RolledBack)
        .expect_err("public status table must reject terminal rollback without WAL evidence");

    assert_eq!(rollback_err.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(status_table.status(rolled_back), None);
    assert!(!commit_log.is_rolled_back(rolled_back));
    assert_eq!(commit_log.get_rollback_lsn(rolled_back), None);
    assert_eq!(commit_log.get_rollback_durable_lsn(rolled_back), None);
}

#[test]
fn test_commit_log_rebuild_rejects_seeded_terminal_entry_without_durable_coverage() {
    let wal = MockWal::new();
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal, status_table.clone());

    let committed = TransactionId::new(28);
    commit_log.seed_commit_entry_without_status(CommitLogEntry {
        tx_id: committed,
        commit_lsn: Lsn::new(9),
        durable_lsn: Lsn::new(8),
        timestamp: EngineTimestamp::from_unix_millis(1),
        row_count_affected: 1,
        isolation_level: IsolationLevel::Snapshot,
    });

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
fn test_commit_log_restore_rejects_inconsistent_durability_evidence() {
    let wal = MockWal::new();
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal, status_table);

    let err = commit_log
        .restore_commit_entry(CommitLogEntry {
            tx_id: TransactionId::new(32),
            commit_lsn: Lsn::new(5),
            durable_lsn: Lsn::new(4),
            timestamp: EngineTimestamp::from_unix_millis(1),
            row_count_affected: 1,
            isolation_level: IsolationLevel::Snapshot,
        })
        .expect_err("restore must reject commit entries not covered by durable LSN");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(!commit_log.is_committed(TransactionId::new(32)));
}

#[test]
fn test_commit_log_restore_rejects_rollback_without_durable_coverage() {
    let wal = MockWal::new();
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal, status_table.clone());
    let tx_id = TransactionId::new(33);

    let err = commit_log
        .restore_rollback_entry(RollbackLogEntry {
            tx_id,
            rollback_lsn: Lsn::new(8),
            durable_lsn: Lsn::new(7),
            timestamp: EngineTimestamp::from_unix_millis(1),
            parameter_hash: 0xD00D,
        })
        .expect_err("restore must reject rollback entries not covered by durable LSN");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert_eq!(status_table.status(tx_id), None);
    assert!(!commit_log.is_rolled_back(tx_id));
    assert_eq!(commit_log.get_rollback_lsn(tx_id), None);
}

#[test]
fn test_commit_log_replay_rejects_terminal_records_without_durable_coverage() {
    let wal = MockWal::new();
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal, status_table.clone());

    let committed = TransactionId::new(34);
    let commit_err = commit_log
        .replay_tx_wal_record(TxWalReplayRecord::Commit(CommitLogEntry {
            tx_id: committed,
            commit_lsn: Lsn::new(9),
            durable_lsn: Lsn::new(8),
            timestamp: EngineTimestamp::from_unix_millis(1),
            row_count_affected: 1,
            isolation_level: IsolationLevel::Snapshot,
        }))
        .expect_err("replay must reject commit records not covered by durable LSN");

    assert_eq!(commit_err.kind(), AndromedaErrorKind::Storage);
    assert_eq!(status_table.status(committed), None);
    assert!(!commit_log.is_committed(committed));
    assert_eq!(commit_log.get_commit_lsn(committed), None);

    let rolled_back = TransactionId::new(35);
    let rollback_err = commit_log
        .replay_tx_wal_record(TxWalReplayRecord::Rollback(RollbackLogEntry {
            tx_id: rolled_back,
            rollback_lsn: Lsn::new(10),
            durable_lsn: Lsn::new(9),
            timestamp: EngineTimestamp::from_unix_millis(2),
            parameter_hash: 0xA11,
        }))
        .expect_err("replay must reject rollback records not covered by durable LSN");

    assert_eq!(rollback_err.kind(), AndromedaErrorKind::Storage);
    assert_eq!(status_table.status(rolled_back), None);
    assert!(!commit_log.is_rolled_back(rolled_back));
    assert_eq!(commit_log.get_rollback_lsn(rolled_back), None);
}

#[test]
fn test_commit_log_replay_advances_in_flight_status_with_durable_terminal_evidence()
-> AndromedaResult<()> {
    let wal = MockWal::new();
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal, status_table.clone());
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
    let wal = MockWal::new();
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal, status_table.clone());
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

#[tokio::test]
async fn test_commit_log_rejects_zero_tx_id() {
    let wal = MockWal::new();
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal, status_table);

    let result = commit_log
        .record_commit(TransactionId::new(0), IsolationLevel::Snapshot, 5, 0)
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), AndromedaErrorKind::Transaction);
}

#[tokio::test]
async fn test_commit_log_gc_candidates() {
    let wal = MockWal::new();
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal, status_table);

    // Record 5 commits
    for i in 1..=5 {
        commit_log
            .record_commit(
                TransactionId::new(i as u64),
                IsolationLevel::Snapshot,
                i as u64,
                0,
            )
            .await
            .unwrap();
    }

    // All LSNs are 1-5, so threshold of Lsn(3) should find tx_ids 1,2
    let candidates = commit_log.gc_candidates(Lsn::new(3));
    assert_eq!(candidates.len(), 2);
    assert!(candidates.contains(&TransactionId::new(1)));
    assert!(candidates.contains(&TransactionId::new(2)));
}

#[tokio::test]
async fn test_commit_log_verify_durability() {
    let wal = MockWal::new();
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal, status_table);

    let tx_id = TransactionId::new(1);
    commit_log
        .record_commit(tx_id, IsolationLevel::Snapshot, 5, 0)
        .await
        .unwrap();

    // Verify succeeds for committed transaction
    assert!(commit_log.verify_durability(tx_id).is_ok());

    // Verify fails for uncommitted transaction
    let result = commit_log.verify_durability(TransactionId::new(999));
    assert!(result.is_err());
}

#[tokio::test]
async fn test_commit_log_isolation_levels() {
    let wal = MockWal::new();
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal, status_table);

    let snapshot_entry = commit_log
        .record_commit(TransactionId::new(1), IsolationLevel::Snapshot, 10, 0)
        .await
        .unwrap();
    assert_eq!(snapshot_entry.isolation_level, IsolationLevel::Snapshot);

    let serializable_entry = commit_log
        .record_commit(TransactionId::new(2), IsolationLevel::Serializable, 20, 0)
        .await
        .unwrap();
    assert_eq!(
        serializable_entry.isolation_level,
        IsolationLevel::Serializable
    );
}
