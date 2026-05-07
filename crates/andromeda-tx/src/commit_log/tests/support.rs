use super::super::*;

use std::sync::{Arc, Mutex};

pub(super) struct MockWal {
    pub(super) records: Mutex<Vec<Lsn>>,
    pub(super) durable_lsn: Mutex<Lsn>,
    short_flush_lsn: Option<Lsn>,
}

impl MockWal {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            records: Mutex::new(Vec::new()),
            durable_lsn: Mutex::new(Lsn::new(0)),
            short_flush_lsn: None,
        })
    }

    pub(super) fn short_flush(short_flush_lsn: Lsn) -> Arc<Self> {
        Arc::new(Self {
            records: Mutex::new(Vec::new()),
            durable_lsn: Mutex::new(Lsn::new(0)),
            short_flush_lsn: Some(short_flush_lsn),
        })
    }

    pub(super) fn short_flush_with_existing_record(short_flush_lsn: Lsn) -> Arc<Self> {
        let wal = Self::short_flush(short_flush_lsn);
        wal.records.lock().unwrap().push(short_flush_lsn);
        wal
    }
}

pub(super) fn setup_commit_log() -> (Arc<MockWal>, Arc<TransactionStatusTable>, CommitLogManager) {
    setup_commit_log_with_wal(MockWal::new())
}

pub(super) fn setup_commit_log_with_wal(
    wal: Arc<MockWal>,
) -> (Arc<MockWal>, Arc<TransactionStatusTable>, CommitLogManager) {
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal.clone(), status_table.clone());
    (wal, status_table, commit_log)
}

pub(super) fn assert_commit_not_published(
    commit_log: &CommitLogManager,
    status_table: &TransactionStatusTable,
    tx_id: TransactionId,
) {
    assert_eq!(status_table.status(tx_id), None);
    assert!(!commit_log.is_committed(tx_id));
    assert_eq!(commit_log.get_commit_lsn(tx_id), None);
    assert_eq!(commit_log.get_commit_durable_lsn(tx_id), None);
}

pub(super) fn assert_rollback_not_published(
    commit_log: &CommitLogManager,
    status_table: &TransactionStatusTable,
    tx_id: TransactionId,
) {
    assert_eq!(status_table.status(tx_id), None);
    assert!(!commit_log.is_rolled_back(tx_id));
    assert_eq!(commit_log.get_rollback_lsn(tx_id), None);
    assert_eq!(commit_log.get_rollback_durable_lsn(tx_id), None);
}

pub(super) fn durable_commit_entry(
    tx_id: TransactionId,
    lsn: Lsn,
    row_count_affected: u64,
    isolation_level: IsolationLevel,
) -> CommitLogEntry {
    commit_entry_with_lsns(tx_id, lsn, lsn, row_count_affected, isolation_level)
}

pub(super) fn commit_entry_with_lsns(
    tx_id: TransactionId,
    commit_lsn: Lsn,
    durable_lsn: Lsn,
    row_count_affected: u64,
    isolation_level: IsolationLevel,
) -> CommitLogEntry {
    CommitLogEntry {
        tx_id,
        commit_lsn,
        durable_lsn,
        timestamp: EngineTimestamp::from_unix_millis(1),
        row_count_affected,
        isolation_level,
    }
}

pub(super) fn durable_rollback_entry(
    tx_id: TransactionId,
    lsn: Lsn,
    parameter_hash: u64,
) -> RollbackLogEntry {
    rollback_entry_with_lsns(tx_id, lsn, lsn, parameter_hash)
}

pub(super) fn rollback_entry_with_lsns(
    tx_id: TransactionId,
    rollback_lsn: Lsn,
    durable_lsn: Lsn,
    parameter_hash: u64,
) -> RollbackLogEntry {
    RollbackLogEntry {
        tx_id,
        rollback_lsn,
        durable_lsn,
        timestamp: EngineTimestamp::from_unix_millis(2),
        parameter_hash,
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
