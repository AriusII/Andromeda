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
use super::rollback::RollbackLogEntry;
use super::wal::{InvocationWal, encode_commit_payload, encode_rollback_payload};

mod recovery;
mod validation;

use validation::{
    conflicting_terminal_record, validate_durable_flush_covers_record,
    validate_terminal_status_compatible, validate_transaction_id,
};

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
        if let Some(existing) = self.existing_commit_entry(tx_id)? {
            self.restore_commit_status_from_entry(&existing)?;
            return Ok(existing);
        }
        self.reject_commit_after_rollback(tx_id)?;

        let payload = encode_commit_payload(isolation_level, affected_rows, parameter_hash);
        let (commit_lsn, durable_lsn) = self
            .append_and_flush_terminal_record(WalRecordKind::TxCommit, tx_id, &payload, "commit")
            .await?;

        let entry = CommitLogEntry::from_durable_wal(
            tx_id,
            commit_lsn,
            durable_lsn,
            self.clock.now(),
            affected_rows,
            isolation_level,
        )?;

        self.commit_entries.insert(tx_id, entry.clone());
        self.restore_commit_status_from_entry(&entry)?;

        Ok(entry)
    }

    pub async fn record_rollback(
        &self,
        tx_id: TransactionId,
        parameter_hash: u64,
    ) -> AndromedaResult<RollbackLogEntry> {
        validate_transaction_id(tx_id, "cannot rollback transaction with zero ID")?;

        let _decision = self.lifecycle_lock.lock().await;
        if let Some(existing) = self.existing_rollback_entry(tx_id)? {
            self.restore_rollback_status_from_entry(&existing)?;
            return Ok(existing);
        }
        self.reject_rollback_after_commit(tx_id)?;

        let payload = encode_rollback_payload(parameter_hash);
        let (rollback_lsn, durable_lsn) = self
            .append_and_flush_terminal_record(
                WalRecordKind::TxRollback,
                tx_id,
                &payload,
                "rollback",
            )
            .await?;

        let entry = RollbackLogEntry::from_durable_wal(
            tx_id,
            rollback_lsn,
            durable_lsn,
            self.clock.now(),
            parameter_hash,
        )?;

        self.rollback_entries.insert(tx_id, entry.clone());
        self.restore_rollback_status_from_entry(&entry)?;

        Ok(entry)
    }

    pub fn is_committed(&self, tx_id: TransactionId) -> bool {
        self.rebuild_status_for_transaction(tx_id);
        if self.valid_commit_entry(tx_id).is_none() {
            return false;
        }
        matches!(
            self.status_table.get_status(tx_id),
            Some(TransactionStatus::Committed)
        )
    }

    pub fn is_rolled_back(&self, tx_id: TransactionId) -> bool {
        self.rebuild_status_for_transaction(tx_id);
        if self.valid_rollback_entry(tx_id).is_none() {
            return false;
        }
        matches!(
            self.status_table.get_status(tx_id),
            Some(TransactionStatus::RolledBack)
        )
    }

    pub fn get_commit_lsn(&self, tx_id: TransactionId) -> Option<Lsn> {
        self.valid_commit_entry(tx_id).map(|entry| entry.commit_lsn)
    }

    pub fn get_commit_durable_lsn(&self, tx_id: TransactionId) -> Option<Lsn> {
        self.valid_commit_entry(tx_id)
            .map(|entry| entry.durable_lsn)
    }

    pub fn get_rollback_lsn(&self, tx_id: TransactionId) -> Option<Lsn> {
        self.valid_rollback_entry(tx_id)
            .map(|entry| entry.rollback_lsn)
    }

    pub fn get_rollback_durable_lsn(&self, tx_id: TransactionId) -> Option<Lsn> {
        self.valid_rollback_entry(tx_id)
            .map(|entry| entry.durable_lsn)
    }

    pub fn get_commit_timestamp(&self, tx_id: TransactionId) -> Option<EngineTimestamp> {
        self.valid_commit_entry(tx_id).map(|entry| entry.timestamp)
    }

    pub fn get_affected_rows(&self, tx_id: TransactionId) -> Option<u64> {
        self.valid_commit_entry(tx_id)
            .map(|entry| entry.row_count_affected)
    }

    pub fn gc_candidates(&self, min_active_snapshot_lsn: Lsn) -> Vec<TransactionId> {
        self.commit_entries
            .iter()
            .filter_map(|entry| {
                if entry.value().validate_durable_evidence().is_ok()
                    && entry.value().is_before(min_active_snapshot_lsn)
                {
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
        entry.validate_durable_evidence()?;
        if self.rollback_entries.contains_key(&entry.tx_id) {
            return Err(conflicting_terminal_record());
        }
        validate_terminal_status_compatible(
            &self.status_table,
            entry.tx_id,
            TransactionStatus::Committed,
        )?;

        if let Some(existing) = self.commit_entries.get(&entry.tx_id) {
            if *existing != entry {
                return Err(conflicting_terminal_record());
            }
        } else {
            self.commit_entries.insert(entry.tx_id, entry.clone());
        }

        self.restore_commit_status_from_entry(&entry)
    }

    pub fn restore_rollback_entry(&self, entry: RollbackLogEntry) -> AndromedaResult<()> {
        validate_transaction_id(entry.tx_id, "cannot restore rollback entry with zero ID")?;
        entry.validate_durable_evidence()?;
        if self.commit_entries.contains_key(&entry.tx_id) {
            return Err(conflicting_terminal_record());
        }
        validate_terminal_status_compatible(
            &self.status_table,
            entry.tx_id,
            TransactionStatus::RolledBack,
        )?;

        if let Some(existing) = self.rollback_entries.get(&entry.tx_id) {
            if *existing != entry {
                return Err(conflicting_terminal_record());
            }
        } else {
            self.rollback_entries.insert(entry.tx_id, entry.clone());
        }

        self.restore_rollback_status_from_entry(&entry)
    }

    pub fn verify_durability(&self, tx_id: TransactionId) -> AndromedaResult<()> {
        self.commit_entries
            .get(&tx_id)
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Internal,
                    "Commit entry not found during durability verification",
                )
            })?
            .validate_durable_evidence()
    }

    pub fn verify_rollback_durability(&self, tx_id: TransactionId) -> AndromedaResult<()> {
        self.rollback_entries
            .get(&tx_id)
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Internal,
                    "Rollback entry not found during durability verification",
                )
            })?
            .validate_durable_evidence()
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

    fn restore_commit_status_from_entry(&self, entry: &CommitLogEntry) -> AndromedaResult<()> {
        entry.validate_durable_evidence()?;
        self.status_table.record_commit_from_durable_evidence(
            entry.tx_id,
            entry.commit_lsn,
            entry.durable_lsn,
        )
    }

    fn restore_rollback_status_from_entry(&self, entry: &RollbackLogEntry) -> AndromedaResult<()> {
        entry.validate_durable_evidence()?;
        self.status_table.record_rollback_from_durable_evidence(
            entry.tx_id,
            entry.rollback_lsn,
            entry.durable_lsn,
        )
    }

    async fn append_and_flush_terminal_record(
        &self,
        kind: WalRecordKind,
        tx_id: TransactionId,
        payload: &[u8],
        record_name: &'static str,
    ) -> AndromedaResult<(Lsn, Lsn)> {
        let record_lsn = self.wal_manager.append(kind, Some(tx_id), payload).await?;

        // Do not make terminal transaction state visible before this flush covers the record LSN.
        let durable_lsn = self.wal_manager.flush_through(record_lsn).await?;
        validate_durable_flush_covers_record(durable_lsn, record_lsn, record_name)?;

        Ok((record_lsn, durable_lsn))
    }

    fn rebuild_status_for_transaction(&self, tx_id: TransactionId) {
        if self.status_table.status(tx_id).is_some() {
            return;
        }

        if self.valid_commit_entry(tx_id).is_some() && !self.rollback_entries.contains_key(&tx_id) {
            if let Some(entry) = self.valid_commit_entry(tx_id) {
                let _ = self.restore_commit_status_from_entry(&entry);
            }
        } else if !self.commit_entries.contains_key(&tx_id)
            && let Some(entry) = self.valid_rollback_entry(tx_id)
        {
            let _ = self.restore_rollback_status_from_entry(&entry);
        }
    }

    fn valid_commit_entry(&self, tx_id: TransactionId) -> Option<CommitLogEntry> {
        let entry = self.commit_entries.get(&tx_id)?;
        entry.validate_durable_evidence().ok()?;
        Some(entry.clone())
    }

    fn valid_rollback_entry(&self, tx_id: TransactionId) -> Option<RollbackLogEntry> {
        let entry = self.rollback_entries.get(&tx_id)?;
        entry.validate_durable_evidence().ok()?;
        Some(entry.clone())
    }

    fn existing_commit_entry(
        &self,
        tx_id: TransactionId,
    ) -> AndromedaResult<Option<CommitLogEntry>> {
        let Some(entry) = self.commit_entries.get(&tx_id) else {
            return Ok(None);
        };
        let entry = entry.clone();
        entry.validate_durable_evidence()?;
        Ok(Some(entry))
    }

    fn existing_rollback_entry(
        &self,
        tx_id: TransactionId,
    ) -> AndromedaResult<Option<RollbackLogEntry>> {
        let Some(entry) = self.rollback_entries.get(&tx_id) else {
            return Ok(None);
        };
        let entry = entry.clone();
        entry.validate_durable_evidence()?;
        Ok(Some(entry))
    }
}
