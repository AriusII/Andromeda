//! Transaction commit-log manager.
//!
//! The durable record contracts live in `andromeda-transaction-log`. This
//! module owns the live append-flush-publish coordinator that mirrors durable
//! terminal evidence into the MVCC status table.

use andromeda_error::AndromedaResult;
use andromeda_time::{Clock, EngineTimestamp, SystemClock};
use andromeda_types::TransactionId;
use std::sync::Arc;

pub use andromeda_mvcc::{TransactionStatus, TransactionStatusTable};
pub use andromeda_transaction_log::{
    CommitLogEntry, InvocationWal, IsolationLevel, Lsn, RollbackLogEntry, TransactionLogStatus,
    TransactionStatusRebuild, TransactionStatusStore, TxWalReplayAction, TxWalReplayRecord,
    TxWalReplaySummary, WalRecordKind,
};

/// Compatibility facade that adapts the MVCC status table to the owner crate's
/// generic commit-log manager.
pub struct CommitLogManager {
    inner: andromeda_transaction_log::CommitLogManager<MvccCommitStatusStore>,
}

impl CommitLogManager {
    pub fn new(
        wal_manager: Arc<dyn InvocationWal>,
        status_table: Arc<TransactionStatusTable>,
    ) -> Self {
        Self::with_clock(wal_manager, status_table, Arc::new(SystemClock))
    }

    pub fn with_clock(
        wal_manager: Arc<dyn InvocationWal>,
        status_table: Arc<TransactionStatusTable>,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Self {
        Self {
            inner: andromeda_transaction_log::CommitLogManager::with_clock(
                wal_manager,
                Arc::new(MvccCommitStatusStore { status_table }),
                clock,
            ),
        }
    }

    pub async fn record_commit(
        &self,
        tx_id: TransactionId,
        isolation_level: IsolationLevel,
        affected_rows: u64,
        parameter_hash: u64,
    ) -> AndromedaResult<CommitLogEntry> {
        self.inner
            .record_commit(tx_id, isolation_level, affected_rows, parameter_hash)
            .await
    }

    pub async fn record_rollback(
        &self,
        tx_id: TransactionId,
        parameter_hash: u64,
    ) -> AndromedaResult<RollbackLogEntry> {
        self.inner.record_rollback(tx_id, parameter_hash).await
    }

    pub fn is_committed(&self, tx_id: TransactionId) -> bool {
        self.inner.is_committed(tx_id)
    }

    pub fn is_rolled_back(&self, tx_id: TransactionId) -> bool {
        self.inner.is_rolled_back(tx_id)
    }

    pub fn get_commit_lsn(&self, tx_id: TransactionId) -> Option<Lsn> {
        self.inner.get_commit_lsn(tx_id)
    }

    pub fn get_commit_durable_lsn(&self, tx_id: TransactionId) -> Option<Lsn> {
        self.inner.get_commit_durable_lsn(tx_id)
    }

    pub fn get_rollback_lsn(&self, tx_id: TransactionId) -> Option<Lsn> {
        self.inner.get_rollback_lsn(tx_id)
    }

    pub fn get_rollback_durable_lsn(&self, tx_id: TransactionId) -> Option<Lsn> {
        self.inner.get_rollback_durable_lsn(tx_id)
    }

    pub fn get_commit_timestamp(&self, tx_id: TransactionId) -> Option<EngineTimestamp> {
        self.inner.get_commit_timestamp(tx_id)
    }

    pub fn get_affected_rows(&self, tx_id: TransactionId) -> Option<u64> {
        self.inner.get_affected_rows(tx_id)
    }

    pub fn gc_candidates(&self, min_active_snapshot_lsn: Lsn) -> Vec<TransactionId> {
        self.inner.gc_candidates(min_active_snapshot_lsn)
    }

    pub fn gc_remove(&self, tx_ids: impl Iterator<Item = TransactionId>) -> usize {
        self.inner.gc_remove(tx_ids)
    }

    pub fn restore_commit_entry(&self, entry: CommitLogEntry) -> AndromedaResult<()> {
        self.inner.restore_commit_entry(entry)
    }

    pub fn restore_rollback_entry(&self, entry: RollbackLogEntry) -> AndromedaResult<()> {
        self.inner.restore_rollback_entry(entry)
    }

    pub fn verify_durability(&self, tx_id: TransactionId) -> AndromedaResult<()> {
        self.inner.verify_durability(tx_id)
    }

    pub fn verify_rollback_durability(&self, tx_id: TransactionId) -> AndromedaResult<()> {
        self.inner.verify_rollback_durability(tx_id)
    }

    pub fn replay_tx_wal_record(
        &self,
        record: TxWalReplayRecord,
    ) -> AndromedaResult<TxWalReplayAction> {
        self.inner.replay_tx_wal_record(record)
    }

    pub fn reconstruct_from_tx_wal_replay<I>(
        &self,
        records: I,
    ) -> AndromedaResult<TxWalReplaySummary>
    where
        I: IntoIterator<Item = TxWalReplayRecord>,
    {
        self.inner.reconstruct_from_tx_wal_replay(records)
    }

    pub fn rebuild_status_from_records(&self) -> AndromedaResult<TransactionStatusRebuild> {
        self.inner.rebuild_status_from_records()
    }
}

struct MvccCommitStatusStore {
    status_table: Arc<TransactionStatusTable>,
}

impl TransactionStatusStore for MvccCommitStatusStore {
    fn status(&self, tx_id: TransactionId) -> Option<TransactionLogStatus> {
        self.status_table.status(tx_id).map(mvcc_status_to_log)
    }

    fn record_commit_from_durable_evidence(
        &self,
        tx_id: TransactionId,
        commit_lsn: Lsn,
        durable_lsn: Lsn,
    ) -> AndromedaResult<()> {
        self.status_table
            .record_commit_from_durable_evidence(tx_id, commit_lsn, durable_lsn)
    }

    fn record_rollback_from_durable_evidence(
        &self,
        tx_id: TransactionId,
        rollback_lsn: Lsn,
        durable_lsn: Lsn,
    ) -> AndromedaResult<()> {
        self.status_table
            .record_rollback_from_durable_evidence(tx_id, rollback_lsn, durable_lsn)
    }

    fn restore_terminal_from_validated_replay(
        &self,
        tx_id: TransactionId,
        status: TransactionLogStatus,
    ) -> AndromedaResult<()> {
        self.status_table
            .restore_terminal_from_validated_replay(tx_id, log_status_to_mvcc(status))
    }
}

fn mvcc_status_to_log(status: TransactionStatus) -> TransactionLogStatus {
    match status {
        TransactionStatus::InFlight => TransactionLogStatus::InFlight,
        TransactionStatus::Committed => TransactionLogStatus::Committed,
        TransactionStatus::RolledBack => TransactionLogStatus::RolledBack,
    }
}

fn log_status_to_mvcc(status: TransactionLogStatus) -> TransactionStatus {
    match status {
        TransactionLogStatus::InFlight => TransactionStatus::InFlight,
        TransactionLogStatus::Committed => TransactionStatus::Committed,
        TransactionLogStatus::RolledBack => TransactionStatus::RolledBack,
    }
}
