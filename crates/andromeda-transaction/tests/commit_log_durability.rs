//! Transaction commit-log durability coverage against the owner crate.

#[cfg(test)]
mod tests {
    use andromeda_error::AndromedaResult;
    use andromeda_mvcc::{TransactionStatus, TransactionStatusTable};
    use andromeda_transaction_log::{
        CommitLogManager, IsolationLevel, Lsn, TransactionLogStatus, TransactionStatusStore,
        WalRecordKind,
    };
    use andromeda_types::TransactionId;
    use std::sync::Arc;

    type MockWalRecord = (WalRecordKind, Option<TransactionId>, Vec<u8>);

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
    }

    #[async_trait::async_trait]
    impl andromeda_transaction_log::InvocationWal for MockWal {
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

    struct MvccStatusStore {
        table: Arc<TransactionStatusTable>,
    }

    impl MvccStatusStore {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                table: Arc::new(TransactionStatusTable::new()),
            })
        }
    }

    impl TransactionStatusStore for MvccStatusStore {
        fn status(&self, tx_id: TransactionId) -> Option<TransactionLogStatus> {
            self.table.status(tx_id).map(|status| match status {
                TransactionStatus::InFlight => TransactionLogStatus::InFlight,
                TransactionStatus::Committed => TransactionLogStatus::Committed,
                TransactionStatus::RolledBack => TransactionLogStatus::RolledBack,
            })
        }

        fn record_commit_from_durable_evidence(
            &self,
            tx_id: TransactionId,
            commit_lsn: Lsn,
            durable_lsn: Lsn,
        ) -> AndromedaResult<()> {
            self.table
                .record_commit_from_durable_evidence(tx_id, commit_lsn, durable_lsn)
        }

        fn record_rollback_from_durable_evidence(
            &self,
            tx_id: TransactionId,
            rollback_lsn: Lsn,
            durable_lsn: Lsn,
        ) -> AndromedaResult<()> {
            self.table
                .record_rollback_from_durable_evidence(tx_id, rollback_lsn, durable_lsn)
        }

        fn restore_terminal_from_validated_replay(
            &self,
            tx_id: TransactionId,
            status: TransactionLogStatus,
        ) -> AndromedaResult<()> {
            self.table.restore_terminal_from_validated_replay(
                tx_id,
                match status {
                    TransactionLogStatus::InFlight => TransactionStatus::InFlight,
                    TransactionLogStatus::Committed => TransactionStatus::Committed,
                    TransactionLogStatus::RolledBack => TransactionStatus::RolledBack,
                },
            )
        }
    }

    #[tokio::test]
    async fn commit_log_records_durable_commit() {
        let wal = MockWal::new();
        let status_table = MvccStatusStore::new();
        let commit_log = Arc::new(CommitLogManager::new(wal, status_table));

        let tx_id = TransactionId::new(1);
        let result = commit_log
            .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
            .await;

        assert!(result.is_ok());
        assert!(commit_log.is_committed(tx_id));
    }

    #[tokio::test]
    async fn commit_log_reuses_existing_commit_entry() {
        let wal = MockWal::new();
        let status_table = MvccStatusStore::new();
        let commit_log = Arc::new(CommitLogManager::new(wal.clone(), status_table));

        let first = commit_log
            .record_commit(TransactionId::new(1), IsolationLevel::Snapshot, 10, 0)
            .await
            .unwrap();
        let second = commit_log
            .record_commit(TransactionId::new(1), IsolationLevel::Serializable, 99, 1)
            .await
            .unwrap();

        assert_eq!(first.commit_lsn, second.commit_lsn);
        assert_eq!(wal.records.lock().unwrap().len(), 1);
        assert!(commit_log.is_committed(TransactionId::new(1)));
    }

    #[tokio::test]
    async fn commit_log_gets_commit_lsn() {
        let wal = MockWal::new();
        let status_table = MvccStatusStore::new();
        let commit_log = Arc::new(CommitLogManager::new(wal, status_table));

        let tx_id = TransactionId::new(1);
        commit_log
            .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
            .await
            .unwrap();

        assert_eq!(commit_log.get_commit_lsn(tx_id), Some(Lsn::new(1)));
    }
}
