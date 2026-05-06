use super::*;

use std::sync::Arc;

/// Mock WAL for testing commit log without async I/O
struct MockWal {
    records: std::sync::Mutex<Vec<Lsn>>,
    durable_lsn: std::sync::Mutex<Lsn>,
}

impl MockWal {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            records: std::sync::Mutex::new(Vec::new()),
            durable_lsn: std::sync::Mutex::new(Lsn::new(0)),
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
        *durable = lsn;
        Ok(lsn)
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
    assert_eq!(entry.parameter_hash, 0xCAFE);
    assert!(commit_log.is_rolled_back(tx_id));
    assert_eq!(
        status_table.status(tx_id),
        Some(TransactionStatus::RolledBack)
    );
    assert_eq!(*wal.durable_lsn.lock().unwrap(), entry.rollback_lsn);
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
            timestamp: EngineTimestamp::from_unix_millis(1),
            row_count_affected: 2,
            isolation_level: IsolationLevel::Snapshot,
        })
        .unwrap();
    commit_log
        .restore_rollback_entry(RollbackLogEntry {
            tx_id: rolled_back,
            rollback_lsn: Lsn::new(12),
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
        timestamp: EngineTimestamp::from_unix_millis(1),
        row_count_affected: 2,
        isolation_level: IsolationLevel::Serializable,
    });
    recovered.seed_rollback_entry_without_status(RollbackLogEntry {
        tx_id: rolled_back,
        rollback_lsn: Lsn::new(22),
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
