use andromeda_error::{AndromedaErrorKind, AndromedaResult};
use andromeda_transaction::{CommitLogManager, Lsn, TransactionStatusTable, WalRecordKind};
use andromeda_types::TransactionId;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) type TestWalRecord = (WalRecordKind, Option<TransactionId>, Vec<u8>);

/// Test WAL with configurable failure modes for crash scenario testing.
pub(crate) struct TestWal {
    records: std::sync::Mutex<Vec<TestWalRecord>>,
    durable_lsn: std::sync::Mutex<Lsn>,
    next_lsn: AtomicU64,
    fail_flush_after: Option<u64>,
}

impl TestWal {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            records: std::sync::Mutex::new(Vec::new()),
            durable_lsn: std::sync::Mutex::new(Lsn::new(0)),
            next_lsn: AtomicU64::new(1),
            fail_flush_after: None,
        })
    }

    pub(crate) fn with_fail_flush_after(fail_after: u64) -> Arc<Self> {
        Arc::new(Self {
            records: std::sync::Mutex::new(Vec::new()),
            durable_lsn: std::sync::Mutex::new(Lsn::new(0)),
            next_lsn: AtomicU64::new(1),
            fail_flush_after: Some(fail_after),
        })
    }

    pub(crate) fn record_count(&self) -> usize {
        self.records.lock().unwrap().len()
    }

    pub(crate) fn get_durable_lsn(&self) -> Lsn {
        *self.durable_lsn.lock().unwrap()
    }

    pub(crate) fn get_records(&self) -> Vec<TestWalRecord> {
        self.records.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl andromeda_transaction_log::InvocationWal for TestWal {
    async fn append(
        &self,
        kind: WalRecordKind,
        transaction_id: Option<TransactionId>,
        payload: &[u8],
    ) -> AndromedaResult<Lsn> {
        let mut records = self.records.lock().unwrap();
        let lsn_val = self.next_lsn.fetch_add(1, Ordering::Release);
        let lsn = Lsn::new(lsn_val);
        records.push((kind, transaction_id, payload.to_vec()));
        Ok(lsn)
    }

    async fn flush_through(&self, lsn: Lsn) -> AndromedaResult<Lsn> {
        if let Some(fail_after) = self.fail_flush_after
            && lsn.get() > fail_after
        {
            return Err(andromeda_error::AndromedaError::new(
                AndromedaErrorKind::Storage,
                "Simulated WAL flush failure",
            ));
        }

        let mut durable = self.durable_lsn.lock().unwrap();
        *durable = lsn;
        Ok(lsn)
    }
}

pub(crate) fn setup_commit_log() -> (Arc<TestWal>, Arc<TransactionStatusTable>, CommitLogManager) {
    let wal = TestWal::new();
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal.clone(), status_table.clone());
    (wal, status_table, commit_log)
}

pub(crate) fn setup_commit_log_with_wal(
    wal: Arc<TestWal>,
) -> (Arc<TransactionStatusTable>, CommitLogManager) {
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal, status_table.clone());
    (status_table, commit_log)
}
