use super::super::*;

use crate::status::{TransactionLogStatus, TransactionStatusStore};
pub(super) use crate::{TxWalReplayAction, TxWalReplayRecord};

pub(super) type TransactionStatus = TransactionLogStatus;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Debug, Default)]
pub(super) struct TransactionStatusTable {
    statuses: Mutex<BTreeMap<TransactionId, TransactionStatus>>,
}

impl TransactionStatusTable {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn status(&self, tx_id: TransactionId) -> Option<TransactionStatus> {
        self.statuses
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&tx_id)
            .copied()
    }

    pub(super) fn record(
        &self,
        tx_id: TransactionId,
        status: TransactionStatus,
    ) -> AndromedaResult<()> {
        if tx_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "transaction status id must not be zero",
            ));
        }
        if status.is_terminal() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "terminal transaction status requires durable WAL evidence",
            ));
        }
        self.lock_statuses().insert(tx_id, status);
        Ok(())
    }

    pub(super) fn record_in_flight(&self, tx_id: TransactionId) -> AndromedaResult<()> {
        self.record(tx_id, TransactionStatus::InFlight)
    }

    pub(super) fn record_rolled_back_after_durable_wal(
        &self,
        tx_id: TransactionId,
        rollback_lsn: Lsn,
        durable_lsn: Lsn,
    ) -> AndromedaResult<()> {
        self.record_terminal_from_durable_evidence(
            tx_id,
            TransactionStatus::RolledBack,
            rollback_lsn,
            durable_lsn,
        )
    }

    fn record_terminal_from_durable_evidence(
        &self,
        tx_id: TransactionId,
        status: TransactionStatus,
        record_lsn: Lsn,
        durable_lsn: Lsn,
    ) -> AndromedaResult<()> {
        if tx_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "transaction status id must not be zero",
            ));
        }
        if record_lsn.get() == 0 || durable_lsn < record_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "terminal transaction status durable LSN must cover record LSN",
            ));
        }
        self.restore_terminal_from_validated_replay(tx_id, status)
    }

    fn lock_statuses(&self) -> MutexGuard<'_, BTreeMap<TransactionId, TransactionStatus>> {
        self.statuses
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl TransactionStatusStore for TransactionStatusTable {
    fn status(&self, tx_id: TransactionId) -> Option<TransactionStatus> {
        self.status(tx_id)
    }

    fn record_commit_from_durable_evidence(
        &self,
        tx_id: TransactionId,
        commit_lsn: Lsn,
        durable_lsn: Lsn,
    ) -> AndromedaResult<()> {
        self.record_terminal_from_durable_evidence(
            tx_id,
            TransactionStatus::Committed,
            commit_lsn,
            durable_lsn,
        )
    }

    fn record_rollback_from_durable_evidence(
        &self,
        tx_id: TransactionId,
        rollback_lsn: Lsn,
        durable_lsn: Lsn,
    ) -> AndromedaResult<()> {
        self.record_terminal_from_durable_evidence(
            tx_id,
            TransactionStatus::RolledBack,
            rollback_lsn,
            durable_lsn,
        )
    }

    fn restore_terminal_from_validated_replay(
        &self,
        tx_id: TransactionId,
        status: TransactionStatus,
    ) -> AndromedaResult<()> {
        let mut statuses = self.lock_statuses();
        match statuses.get(&tx_id).copied() {
            Some(existing) if existing == status => Ok(()),
            Some(TransactionStatus::InFlight) | None => {
                statuses.insert(tx_id, status);
                Ok(())
            },
            Some(_) => Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "conflicting transaction terminal status",
            )),
        }
    }
}

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

pub(super) fn setup_commit_log() -> (
    Arc<MockWal>,
    Arc<TransactionStatusTable>,
    CommitLogManager<TransactionStatusTable>,
) {
    setup_commit_log_with_wal(MockWal::new())
}

pub(super) fn setup_commit_log_with_wal(
    wal: Arc<MockWal>,
) -> (
    Arc<MockWal>,
    Arc<TransactionStatusTable>,
    CommitLogManager<TransactionStatusTable>,
) {
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(wal.clone(), status_table.clone());
    (wal, status_table, commit_log)
}

pub(super) fn assert_commit_not_published(
    commit_log: &CommitLogManager<TransactionStatusTable>,
    status_table: &TransactionStatusTable,
    tx_id: TransactionId,
) {
    assert_eq!(status_table.status(tx_id), None);
    assert!(!commit_log.is_committed(tx_id));
    assert_eq!(commit_log.get_commit_lsn(tx_id), None);
    assert_eq!(commit_log.get_commit_durable_lsn(tx_id), None);
}

pub(super) fn assert_rollback_not_published(
    commit_log: &CommitLogManager<TransactionStatusTable>,
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
    let TxWalReplayRecord::Commit(entry) = TxWalReplayRecord::commit_with_durable_lsn(
        tx_id,
        commit_lsn,
        durable_lsn,
        EngineTimestamp::from_unix_millis(1),
        row_count_affected,
        isolation_level,
    ) else {
        unreachable!("commit replay constructor must return a commit entry");
    };

    entry
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
    let TxWalReplayRecord::Rollback(entry) = TxWalReplayRecord::rollback_with_durable_lsn(
        tx_id,
        rollback_lsn,
        durable_lsn,
        EngineTimestamp::from_unix_millis(2),
        parameter_hash,
    ) else {
        unreachable!("rollback replay constructor must return a rollback entry");
    };

    entry
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
