use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, Clock, EngineTimestamp, SystemClock,
    TransactionId,
};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;

use crate::Lsn;
use crate::mvcc_status::{TransactionStatus, TransactionStatusTable};

use super::entry::{CommitLogEntry, IsolationLevel, WalRecordKind};
use super::replay::{TxWalReplayAction, TxWalReplayRecord, TxWalReplaySummary};
use super::rollback::RollbackLogEntry;
use super::status_rebuild::TransactionStatusRebuild;
use super::wal::{InvocationWal, encode_commit_payload, encode_rollback_payload};

/// Manager for transaction terminal states linked to WAL durability.
pub struct CommitLogManager {
    wal_manager: Arc<dyn InvocationWal>,
    status_table: Arc<TransactionStatusTable>,
    commit_entries: Arc<DashMap<TransactionId, CommitLogEntry>>,
    rollback_entries: Arc<DashMap<TransactionId, RollbackLogEntry>>,
    lifecycle_lock: AsyncMutex<()>,
    clock: Arc<dyn Clock + Send + Sync>,
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
            wal_manager,
            status_table,
            commit_entries: Arc::new(DashMap::new()),
            rollback_entries: Arc::new(DashMap::new()),
            lifecycle_lock: AsyncMutex::new(()),
            clock,
        }
    }

    pub async fn record_commit(
        &self,
        tx_id: TransactionId,
        isolation_level: IsolationLevel,
        affected_rows: u64,
        parameter_hash: u64,
    ) -> AndromedaResult<CommitLogEntry> {
        validate_transaction_id(tx_id, "cannot commit transaction with zero ID")?;

        let _decision = self.lifecycle_lock.lock().await;
        if let Some(existing) = self.commit_entries.get(&tx_id) {
            return Ok(existing.clone());
        }
        self.reject_commit_after_rollback(tx_id)?;

        let payload = encode_commit_payload(isolation_level, affected_rows, parameter_hash);
        let commit_lsn = self
            .wal_manager
            .append(WalRecordKind::TxCommit, Some(tx_id), &payload)
            .await?;

        // Do not make the transaction visible before this flush returns.
        self.wal_manager.flush_through(commit_lsn).await?;

        let entry = CommitLogEntry {
            tx_id,
            commit_lsn,
            timestamp: self.clock.now(),
            row_count_affected: affected_rows,
            isolation_level,
        };

        self.commit_entries.insert(tx_id, entry.clone());
        self.status_table.set_committed(tx_id)?;

        Ok(entry)
    }

    pub async fn record_rollback(
        &self,
        tx_id: TransactionId,
        parameter_hash: u64,
    ) -> AndromedaResult<RollbackLogEntry> {
        validate_transaction_id(tx_id, "cannot rollback transaction with zero ID")?;

        let _decision = self.lifecycle_lock.lock().await;
        if let Some(existing) = self.rollback_entries.get(&tx_id) {
            return Ok(existing.clone());
        }
        self.reject_rollback_after_commit(tx_id)?;

        let payload = encode_rollback_payload(parameter_hash);
        let rollback_lsn = self
            .wal_manager
            .append(WalRecordKind::TxRollback, Some(tx_id), &payload)
            .await?;

        self.wal_manager.flush_through(rollback_lsn).await?;

        let entry = RollbackLogEntry {
            tx_id,
            rollback_lsn,
            timestamp: self.clock.now(),
            parameter_hash,
        };

        self.rollback_entries.insert(tx_id, entry.clone());
        self.status_table
            .record(tx_id, TransactionStatus::RolledBack)?;

        Ok(entry)
    }

    pub fn is_committed(&self, tx_id: TransactionId) -> bool {
        self.rebuild_status_for_transaction(tx_id);
        matches!(
            self.status_table.get_status(tx_id),
            Some(TransactionStatus::Committed)
        )
    }

    pub fn is_rolled_back(&self, tx_id: TransactionId) -> bool {
        self.rebuild_status_for_transaction(tx_id);
        matches!(
            self.status_table.get_status(tx_id),
            Some(TransactionStatus::RolledBack)
        )
    }

    pub fn get_commit_lsn(&self, tx_id: TransactionId) -> Option<Lsn> {
        self.commit_entries
            .get(&tx_id)
            .map(|entry| entry.commit_lsn)
    }

    pub fn get_rollback_lsn(&self, tx_id: TransactionId) -> Option<Lsn> {
        self.rollback_entries
            .get(&tx_id)
            .map(|entry| entry.rollback_lsn)
    }

    pub fn get_commit_timestamp(&self, tx_id: TransactionId) -> Option<EngineTimestamp> {
        self.commit_entries.get(&tx_id).map(|entry| entry.timestamp)
    }

    pub fn get_affected_rows(&self, tx_id: TransactionId) -> Option<u64> {
        self.commit_entries
            .get(&tx_id)
            .map(|entry| entry.row_count_affected)
    }

    pub fn gc_candidates(&self, min_active_snapshot_lsn: Lsn) -> Vec<TransactionId> {
        self.commit_entries
            .iter()
            .filter_map(|entry| {
                if entry.value().is_before(min_active_snapshot_lsn) {
                    Some(entry.value().tx_id)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn gc_remove(&self, tx_ids: impl Iterator<Item = TransactionId>) -> usize {
        let mut count = 0;
        for tx_id in tx_ids {
            if self.commit_entries.remove(&tx_id).is_some() {
                count += 1;
            }
        }
        count
    }

    pub fn restore_commit_entry(&self, entry: CommitLogEntry) -> AndromedaResult<()> {
        validate_transaction_id(entry.tx_id, "cannot restore commit entry with zero ID")?;
        if self.rollback_entries.contains_key(&entry.tx_id) {
            return Err(conflicting_terminal_record());
        }

        if let Some(existing) = self.commit_entries.get(&entry.tx_id) {
            if *existing != entry {
                return Err(conflicting_terminal_record());
            }
        } else {
            self.commit_entries.insert(entry.tx_id, entry.clone());
        }

        self.restore_status_if_absent(entry.tx_id, TransactionStatus::Committed)
    }

    pub fn restore_rollback_entry(&self, entry: RollbackLogEntry) -> AndromedaResult<()> {
        validate_transaction_id(entry.tx_id, "cannot restore rollback entry with zero ID")?;
        if self.commit_entries.contains_key(&entry.tx_id) {
            return Err(conflicting_terminal_record());
        }

        if let Some(existing) = self.rollback_entries.get(&entry.tx_id) {
            if *existing != entry {
                return Err(conflicting_terminal_record());
            }
        } else {
            self.rollback_entries.insert(entry.tx_id, entry.clone());
        }

        self.restore_status_if_absent(entry.tx_id, TransactionStatus::RolledBack)
    }

    pub fn replay_tx_wal_record(
        &self,
        record: TxWalReplayRecord,
    ) -> AndromedaResult<TxWalReplayAction> {
        match record {
            TxWalReplayRecord::Commit(entry) => self.replay_commit_record(entry),
            TxWalReplayRecord::Rollback(entry) => self.replay_rollback_record(entry),
            TxWalReplayRecord::Incomplete { tx_id, .. } => {
                validate_transaction_id(
                    tx_id,
                    "cannot replay incomplete transaction with zero ID",
                )?;
                Ok(TxWalReplayAction::IncompleteIgnored)
            }
        }
    }

    pub fn reconstruct_from_tx_wal_replay<I>(
        &self,
        records: I,
    ) -> AndromedaResult<TxWalReplaySummary>
    where
        I: IntoIterator<Item = TxWalReplayRecord>,
    {
        let mut summary = TxWalReplaySummary::default();
        for record in records {
            summary.record(self.replay_tx_wal_record(record)?);
        }
        Ok(summary)
    }

    pub fn rebuild_status_from_records(&self) -> AndromedaResult<TransactionStatusRebuild> {
        let mut summary = TransactionStatusRebuild {
            committed_restored: 0,
            rolled_back_restored: 0,
            already_present: 0,
        };

        for entry in self.commit_entries.iter() {
            let tx_id = *entry.key();
            if self.rollback_entries.contains_key(&tx_id) {
                return Err(conflicting_terminal_record());
            }
            match self.status_table.status(tx_id) {
                Some(TransactionStatus::Committed) => summary.already_present += 1,
                Some(_) => return Err(conflicting_terminal_record()),
                None => {
                    self.status_table
                        .record(tx_id, TransactionStatus::Committed)?;
                    summary.committed_restored += 1;
                }
            }
        }

        for entry in self.rollback_entries.iter() {
            let tx_id = *entry.key();
            if self.commit_entries.contains_key(&tx_id) {
                return Err(conflicting_terminal_record());
            }
            match self.status_table.status(tx_id) {
                Some(TransactionStatus::RolledBack) => summary.already_present += 1,
                Some(_) => return Err(conflicting_terminal_record()),
                None => {
                    self.status_table
                        .record(tx_id, TransactionStatus::RolledBack)?;
                    summary.rolled_back_restored += 1;
                }
            }
        }

        Ok(summary)
    }

    pub fn verify_durability(&self, tx_id: TransactionId) -> AndromedaResult<()> {
        if self.commit_entries.contains_key(&tx_id) {
            Ok(())
        } else {
            Err(AndromedaError::new(
                AndromedaErrorKind::Internal,
                "Commit entry not found during durability verification",
            ))
        }
    }

    #[cfg(test)]
    pub(super) fn seed_commit_entry_without_status(&self, entry: CommitLogEntry) {
        self.commit_entries.insert(entry.tx_id, entry);
    }

    #[cfg(test)]
    pub(super) fn seed_rollback_entry_without_status(&self, entry: RollbackLogEntry) {
        self.rollback_entries.insert(entry.tx_id, entry);
    }

    fn reject_commit_after_rollback(&self, tx_id: TransactionId) -> AndromedaResult<()> {
        if self.rollback_entries.contains_key(&tx_id)
            || matches!(
                self.status_table.status(tx_id),
                Some(TransactionStatus::RolledBack)
            )
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "cannot commit a transaction with durable rollback evidence",
            ));
        }
        Ok(())
    }

    fn reject_rollback_after_commit(&self, tx_id: TransactionId) -> AndromedaResult<()> {
        if self.commit_entries.contains_key(&tx_id)
            || matches!(
                self.status_table.status(tx_id),
                Some(TransactionStatus::Committed)
            )
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "cannot rollback a transaction with durable commit evidence",
            ));
        }
        Ok(())
    }

    fn restore_status_if_absent(
        &self,
        tx_id: TransactionId,
        status: TransactionStatus,
    ) -> AndromedaResult<()> {
        match self.status_table.status(tx_id) {
            Some(existing) if existing == status => Ok(()),
            Some(_) => Err(conflicting_terminal_record()),
            None => self.status_table.record(tx_id, status),
        }
    }

    fn replay_commit_record(&self, entry: CommitLogEntry) -> AndromedaResult<TxWalReplayAction> {
        validate_transaction_id(entry.tx_id, "cannot replay commit entry with zero ID")?;
        if self.rollback_entries.contains_key(&entry.tx_id) {
            return Err(conflicting_terminal_record());
        }

        let action = if self.commit_entries.contains_key(&entry.tx_id) {
            TxWalReplayAction::DuplicateCommit
        } else {
            self.commit_entries.insert(entry.tx_id, entry.clone());
            TxWalReplayAction::CommitRestored
        };

        self.restore_status_if_absent(entry.tx_id, TransactionStatus::Committed)?;
        Ok(action)
    }

    fn replay_rollback_record(
        &self,
        entry: RollbackLogEntry,
    ) -> AndromedaResult<TxWalReplayAction> {
        validate_transaction_id(entry.tx_id, "cannot replay rollback entry with zero ID")?;
        if self.commit_entries.contains_key(&entry.tx_id) {
            return Err(conflicting_terminal_record());
        }

        let action = if self.rollback_entries.contains_key(&entry.tx_id) {
            TxWalReplayAction::DuplicateRollback
        } else {
            self.rollback_entries.insert(entry.tx_id, entry.clone());
            TxWalReplayAction::RollbackRestored
        };

        self.restore_status_if_absent(entry.tx_id, TransactionStatus::RolledBack)?;
        Ok(action)
    }

    fn rebuild_status_for_transaction(&self, tx_id: TransactionId) {
        if self.status_table.status(tx_id).is_some() {
            return;
        }

        if self.commit_entries.contains_key(&tx_id) && !self.rollback_entries.contains_key(&tx_id) {
            let _ = self
                .status_table
                .record(tx_id, TransactionStatus::Committed);
        } else if self.rollback_entries.contains_key(&tx_id)
            && !self.commit_entries.contains_key(&tx_id)
        {
            let _ = self
                .status_table
                .record(tx_id, TransactionStatus::RolledBack);
        }
    }
}

fn validate_transaction_id(tx_id: TransactionId, message: &'static str) -> AndromedaResult<()> {
    if tx_id.get() == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            message,
        ));
    }
    Ok(())
}

fn conflicting_terminal_record() -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Transaction,
        "conflicting transaction terminal durability records",
    )
}
