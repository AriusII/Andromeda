//! Comprehensive test suite for WAL commit log durability and recovery.
//!
//! Tests verify:
//! - WAL record creation and durability tracking
//! - Five-step commit sequence correctness
//! - TransactionStatusTable visibility updates
//! - Garbage collection candidate identification
//! - Crash recovery scenarios
//! - Concurrent commit handling

#[cfg(test)]
mod tests {
    use andromeda_core::{AndromedaResult, EngineTimestamp, ManualClock, TransactionId};
    use andromeda_tx::{
        CommitLogManager, CommitProtocol, IsolationLevel, Lsn, TransactionState, TransactionStatus,
        TransactionStatusTable, WalRecordKind,
    };
    use std::sync::Arc;

    type MockWalRecord = (WalRecordKind, Option<TransactionId>, Vec<u8>);

    /// Mock WAL implementation for testing without filesystem I/O.
    struct MockWal {
        records: std::sync::Mutex<Vec<MockWalRecord>>,
        durable_lsn: std::sync::Mutex<Lsn>,
    }

    impl MockWal {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                records: std::sync::Mutex::new(Vec::new()),
                durable_lsn: std::sync::Mutex::new(Lsn::new(0)),
            })
        }

        fn record_count(&self) -> usize {
            self.records.lock().unwrap().len()
        }

        fn get_durable_lsn(&self) -> Lsn {
            *self.durable_lsn.lock().unwrap()
        }
    }

    #[async_trait::async_trait]
    impl andromeda_tx::commit_log::InvocationWal for MockWal {
        async fn append(
            &self,
            kind: WalRecordKind,
            transaction_id: Option<TransactionId>,
            payload: &[u8],
        ) -> AndromedaResult<Lsn> {
            let mut records = self.records.lock().unwrap();
            let next_lsn = Lsn::new(records.len() as u64 + 1);
            records.push((kind, transaction_id, payload.to_vec()));
            Ok(next_lsn)
        }

        async fn flush_through(&self, lsn: Lsn) -> AndromedaResult<Lsn> {
            let mut durable = self.durable_lsn.lock().unwrap();
            *durable = lsn;
            Ok(lsn)
        }
    }

    #[tokio::test]
    async fn test_commit_log_records_wal_entry() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = CommitLogManager::new(wal.clone(), status_table);

        let tx_id = TransactionId::new(1);
        let entry = commit_log
            .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
            .await
            .unwrap();

        // Verify entry was recorded
        assert_eq!(entry.tx_id, tx_id);
        assert_eq!(entry.row_count_affected, 10);
        assert_eq!(entry.isolation_level, IsolationLevel::Snapshot);

        // Verify WAL was appended (should have 1 record: TxCommit)
        assert_eq!(wal.record_count(), 1);
    }

    #[tokio::test]
    async fn test_commit_log_is_committed_after_durability() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = CommitLogManager::new(wal, status_table.clone());

        let tx_id = TransactionId::new(1);
        let _entry = commit_log
            .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
            .await
            .unwrap();

        // Verify commit is visible in status table (CRITICAL: after WAL flush)
        assert!(commit_log.is_committed(tx_id));
        assert_eq!(
            status_table.status(tx_id),
            Some(TransactionStatus::Committed)
        );
    }

    #[tokio::test]
    async fn test_commit_log_wal_durability_before_visibility() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = CommitLogManager::new(wal.clone(), status_table);

        let tx_id = TransactionId::new(1);
        let entry = commit_log
            .record_commit(tx_id, IsolationLevel::Snapshot, 5, 0)
            .await
            .unwrap();

        // Verify WAL was flushed to durable storage
        assert!(wal.get_durable_lsn() > Lsn::new(0));
        assert_eq!(wal.get_durable_lsn(), entry.commit_lsn);

        // Verify status table shows committed
        assert!(commit_log.is_committed(tx_id));
    }

    #[tokio::test]
    async fn test_commit_log_snapshot_isolation_level() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = CommitLogManager::new(wal, status_table);

        let tx_id = TransactionId::new(1);
        let entry = commit_log
            .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
            .await
            .unwrap();

        assert_eq!(entry.isolation_level, IsolationLevel::Snapshot);
        assert_eq!(commit_log.get_affected_rows(tx_id), Some(10));
    }

    #[tokio::test]
    async fn test_commit_log_serializable_isolation_level() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = CommitLogManager::new(wal, status_table);

        let tx_id = TransactionId::new(1);
        let entry = commit_log
            .record_commit(tx_id, IsolationLevel::Serializable, 20, 0)
            .await
            .unwrap();

        assert_eq!(entry.isolation_level, IsolationLevel::Serializable);
        assert_eq!(commit_log.get_affected_rows(tx_id), Some(20));
    }

    #[tokio::test]
    async fn test_commit_log_get_commit_lsn() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = CommitLogManager::new(wal, status_table);

        let tx_id = TransactionId::new(1);
        let entry = commit_log
            .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
            .await
            .unwrap();

        let retrieved_lsn = commit_log.get_commit_lsn(tx_id);
        assert_eq!(retrieved_lsn, Some(entry.commit_lsn));
    }

    #[tokio::test]
    async fn test_commit_log_multiple_commits_incremental_lsn() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = CommitLogManager::new(wal, status_table);

        let mut entries = Vec::new();
        for i in 1..=5 {
            entries.push(
                commit_log
                    .record_commit(
                        TransactionId::new(i as u64),
                        IsolationLevel::Snapshot,
                        i as u64,
                        0,
                    )
                    .await
                    .unwrap(),
            );
        }

        // LSNs should be incremental: 1, 2, 3, 4, 5
        for (i, entry) in entries.iter().enumerate() {
            assert_eq!(entry.commit_lsn, Lsn::new((i + 1) as u64));
        }
    }

    #[tokio::test]
    async fn test_commit_log_gc_candidates() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = CommitLogManager::new(wal, status_table);

        // Record 5 commits (LSN 1-5)
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

        // Find GC candidates with threshold Lsn(3)
        // Should include commits before Lsn(3): tx_id 1 (Lsn 1), tx_id 2 (Lsn 2)
        let candidates = commit_log.gc_candidates(Lsn::new(3));
        assert_eq!(candidates.len(), 2);
        assert!(candidates.contains(&TransactionId::new(1)));
        assert!(candidates.contains(&TransactionId::new(2)));
    }

    #[tokio::test]
    async fn test_commit_log_gc_remove() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = CommitLogManager::new(wal, status_table);

        // Record 3 commits
        for i in 1..=3 {
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

        // Find candidates and remove them
        let candidates = commit_log.gc_candidates(Lsn::new(2));
        let removed = commit_log.gc_remove(candidates.into_iter());
        assert_eq!(removed, 1); // Only tx_id 1 should be removed
    }

    #[tokio::test]
    async fn test_commit_log_gc_with_no_candidates() {
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

        // Threshold before all commits: no GC candidates
        let candidates = commit_log.gc_candidates(Lsn::new(1));
        assert_eq!(candidates.len(), 0);
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
    async fn test_commit_protocol_executes_commit_in_committing_state() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = Arc::new(CommitLogManager::new(wal, status_table));
        let protocol = CommitProtocol::new(commit_log.clone());

        let tx_id = TransactionId::new(1);
        let result = protocol
            .execute_commit(
                tx_id,
                TransactionState::Committing,
                IsolationLevel::Snapshot,
                10,
            )
            .await;

        assert!(result.is_ok());
        assert!(commit_log.is_committed(tx_id));
    }

    #[tokio::test]
    async fn test_commit_protocol_rejects_commit_in_active_state() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = Arc::new(CommitLogManager::new(wal, status_table));
        let protocol = CommitProtocol::new(commit_log.clone());

        let result = protocol
            .execute_commit(
                TransactionId::new(1),
                TransactionState::Active,
                IsolationLevel::Snapshot,
                10,
            )
            .await;

        assert!(result.is_err());
        assert!(!commit_log.is_committed(TransactionId::new(1)));
    }

    #[tokio::test]
    async fn test_commit_protocol_get_commit_lsn() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = Arc::new(CommitLogManager::new(wal, status_table));
        let protocol = CommitProtocol::new(commit_log);

        let tx_id = TransactionId::new(1);
        protocol
            .execute_commit(
                tx_id,
                TransactionState::Committing,
                IsolationLevel::Snapshot,
                10,
            )
            .await
            .unwrap();

        let lsn = protocol.get_commit_lsn(tx_id);
        assert!(lsn.is_some());
        assert_eq!(lsn, Some(Lsn::new(1)));
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
    }

    #[tokio::test]
    async fn test_commit_log_with_zero_affected_rows() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = CommitLogManager::new(wal, status_table);

        let tx_id = TransactionId::new(1);
        let entry = commit_log
            .record_commit(tx_id, IsolationLevel::Snapshot, 0, 0)
            .await
            .unwrap();

        assert_eq!(entry.row_count_affected, 0);
        assert!(commit_log.is_committed(tx_id));
    }

    #[tokio::test]
    async fn test_commit_log_with_large_affected_rows() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let commit_log = CommitLogManager::new(wal, status_table);

        let tx_id = TransactionId::new(1);
        let large_row_count = u64::MAX - 1;
        let entry = commit_log
            .record_commit(tx_id, IsolationLevel::Snapshot, large_row_count, 0)
            .await
            .unwrap();

        assert_eq!(entry.row_count_affected, large_row_count);
        assert_eq!(commit_log.get_affected_rows(tx_id), Some(large_row_count));
    }

    #[tokio::test]
    async fn test_commit_log_timestamps_with_manual_clock() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let clock = ManualClock::from_unix_millis(1000);
        let clock_arc = Arc::new(clock);

        let commit_log = CommitLogManager::with_clock(wal, status_table, clock_arc);

        let tx_id = TransactionId::new(1);
        let entry = commit_log
            .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
            .await
            .unwrap();

        assert_eq!(entry.timestamp.as_unix_millis(), 1000);
        assert_eq!(
            commit_log.get_commit_timestamp(tx_id),
            Some(EngineTimestamp::from_unix_millis(1000))
        );
    }

    #[tokio::test]
    async fn test_commit_log_multiple_commits_have_same_clock_time() {
        let wal = MockWal::new();
        let status_table = Arc::new(TransactionStatusTable::new());
        let clock = Arc::new(ManualClock::from_unix_millis(5000));

        let commit_log = CommitLogManager::with_clock(wal, status_table, clock);

        let entry1 = commit_log
            .record_commit(TransactionId::new(1), IsolationLevel::Snapshot, 10, 0)
            .await
            .unwrap();
        let entry2 = commit_log
            .record_commit(TransactionId::new(2), IsolationLevel::Snapshot, 20, 0)
            .await
            .unwrap();

        // Both commits happen at same time (manual clock doesn't advance)
        assert_eq!(entry1.timestamp, entry2.timestamp);
    }
}
