//! Transaction commit-protocol coverage for the commit-log facade.

#[cfg(test)]
mod tests {
    use andromeda_error::AndromedaResult;
    use andromeda_transaction::{
        CommitLogManager, CommitProtocol, IsolationLevel, Lsn, TransactionState,
        TransactionStatusTable, WalRecordKind,
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

        assert_eq!(protocol.get_commit_lsn(tx_id), Some(Lsn::new(1)));
    }
}
