use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, Clock, EngineTimestamp, SystemClock,
    TransactionId,
};
use dashmap::DashMap;
use std::collections::BTreeMap;
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
            let existing = existing.clone();
            existing.validate_durable_evidence()?;
            self.restore_commit_status_from_entry(&existing)?;
            return Ok(existing);
        }
        self.reject_commit_after_rollback(tx_id)?;

        let payload = encode_commit_payload(isolation_level, affected_rows, parameter_hash);
        let commit_lsn = self
            .wal_manager
            .append(WalRecordKind::TxCommit, Some(tx_id), &payload)
            .await?;

        // Do not make the transaction visible before this flush covers the commit LSN.
        let durable_lsn = self.wal_manager.flush_through(commit_lsn).await?;
        validate_durable_flush_covers_record(durable_lsn, commit_lsn, "commit")?;

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
        if let Some(existing) = self.rollback_entries.get(&tx_id) {
            let existing = existing.clone();
            existing.validate_durable_evidence()?;
            self.restore_rollback_status_from_entry(&existing)?;
            return Ok(existing);
        }
        self.reject_rollback_after_commit(tx_id)?;

        let payload = encode_rollback_payload(parameter_hash);
        let rollback_lsn = self
            .wal_manager
            .append(WalRecordKind::TxRollback, Some(tx_id), &payload)
            .await?;

        let durable_lsn = self.wal_manager.flush_through(rollback_lsn).await?;
        validate_durable_flush_covers_record(durable_lsn, rollback_lsn, "rollback")?;

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

    pub fn replay_tx_wal_record(
        &self,
        record: TxWalReplayRecord,
    ) -> AndromedaResult<TxWalReplayAction> {
        match record {
            TxWalReplayRecord::Commit(entry) => self.replay_commit_record(entry),
            TxWalReplayRecord::Rollback(entry) => self.replay_rollback_record(entry),
            TxWalReplayRecord::Incomplete { tx_id, last_lsn } => {
                validate_transaction_id(
                    tx_id,
                    "cannot replay incomplete transaction with zero ID",
                )?;
                if last_lsn.get() == 0 {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Transaction,
                        "cannot replay incomplete transaction with zero LSN",
                    ));
                }
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
        let records: Vec<_> = records.into_iter().collect();
        self.validate_tx_wal_replay_batch(&records)?;

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
            entry.validate_durable_evidence()?;
            if self.rollback_entries.contains_key(&tx_id) {
                return Err(conflicting_terminal_record());
            }
            match self.status_table.status(tx_id) {
                Some(TransactionStatus::Committed) => summary.already_present += 1,
                Some(_) => return Err(conflicting_terminal_record()),
                None => {
                    self.status_table.record_commit_from_durable_evidence(
                        tx_id,
                        entry.commit_lsn,
                        entry.durable_lsn,
                    )?;
                    summary.committed_restored += 1;
                }
            }
        }

        for entry in self.rollback_entries.iter() {
            let tx_id = *entry.key();
            entry.validate_durable_evidence()?;
            if self.commit_entries.contains_key(&tx_id) {
                return Err(conflicting_terminal_record());
            }
            match self.status_table.status(tx_id) {
                Some(TransactionStatus::RolledBack) => summary.already_present += 1,
                Some(_) => return Err(conflicting_terminal_record()),
                None => {
                    self.status_table.record_rollback_from_durable_evidence(
                        tx_id,
                        entry.rollback_lsn,
                        entry.durable_lsn,
                    )?;
                    summary.rolled_back_restored += 1;
                }
            }
        }

        Ok(summary)
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

    fn replay_commit_record(&self, entry: CommitLogEntry) -> AndromedaResult<TxWalReplayAction> {
        validate_transaction_id(entry.tx_id, "cannot replay commit entry with zero ID")?;
        entry.validate_durable_evidence()?;
        if self.rollback_entries.contains_key(&entry.tx_id) {
            return Err(conflicting_terminal_record());
        }
        validate_terminal_status_compatible(
            &self.status_table,
            entry.tx_id,
            TransactionStatus::Committed,
        )?;

        let action = if let Some(existing) = self.commit_entries.get(&entry.tx_id) {
            if *existing != entry {
                return Err(conflicting_terminal_record());
            }
            TxWalReplayAction::DuplicateCommit
        } else {
            self.commit_entries.insert(entry.tx_id, entry.clone());
            TxWalReplayAction::CommitRestored
        };

        self.status_table
            .restore_terminal_from_validated_replay(entry.tx_id, TransactionStatus::Committed)?;
        Ok(action)
    }

    fn replay_rollback_record(
        &self,
        entry: RollbackLogEntry,
    ) -> AndromedaResult<TxWalReplayAction> {
        validate_transaction_id(entry.tx_id, "cannot replay rollback entry with zero ID")?;
        entry.validate_durable_evidence()?;
        if self.commit_entries.contains_key(&entry.tx_id) {
            return Err(conflicting_terminal_record());
        }
        validate_terminal_status_compatible(
            &self.status_table,
            entry.tx_id,
            TransactionStatus::RolledBack,
        )?;

        let action = if let Some(existing) = self.rollback_entries.get(&entry.tx_id) {
            if *existing != entry {
                return Err(conflicting_terminal_record());
            }
            TxWalReplayAction::DuplicateRollback
        } else {
            self.rollback_entries.insert(entry.tx_id, entry.clone());
            TxWalReplayAction::RollbackRestored
        };

        self.status_table
            .restore_terminal_from_validated_replay(entry.tx_id, TransactionStatus::RolledBack)?;
        Ok(action)
    }

    fn validate_tx_wal_replay_batch(&self, records: &[TxWalReplayRecord]) -> AndromedaResult<()> {
        let mut previous_record = None;
        let mut staged_commits = BTreeMap::new();
        let mut staged_rollbacks = BTreeMap::new();

        for record in records {
            validate_replay_lsn_progression(&mut previous_record, record)?;
            match record {
                TxWalReplayRecord::Commit(entry) => {
                    self.validate_replay_commit_candidate(
                        entry,
                        &mut staged_commits,
                        &staged_rollbacks,
                    )?;
                }
                TxWalReplayRecord::Rollback(entry) => {
                    self.validate_replay_rollback_candidate(
                        entry,
                        &staged_commits,
                        &mut staged_rollbacks,
                    )?;
                }
                TxWalReplayRecord::Incomplete { tx_id, last_lsn } => {
                    validate_transaction_id(
                        *tx_id,
                        "cannot replay incomplete transaction with zero ID",
                    )?;
                    if last_lsn.get() == 0 {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Transaction,
                            "cannot replay incomplete transaction with zero LSN",
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    fn validate_replay_commit_candidate(
        &self,
        entry: &CommitLogEntry,
        staged_commits: &mut BTreeMap<TransactionId, CommitLogEntry>,
        staged_rollbacks: &BTreeMap<TransactionId, RollbackLogEntry>,
    ) -> AndromedaResult<()> {
        validate_transaction_id(entry.tx_id, "cannot replay commit entry with zero ID")?;
        entry.validate_durable_evidence()?;
        if self.rollback_entries.contains_key(&entry.tx_id)
            || staged_rollbacks.contains_key(&entry.tx_id)
        {
            return Err(conflicting_terminal_record());
        }
        validate_terminal_status_compatible(
            &self.status_table,
            entry.tx_id,
            TransactionStatus::Committed,
        )?;

        if let Some(existing) = self.commit_entries.get(&entry.tx_id) {
            if *existing != *entry {
                return Err(conflicting_terminal_record());
            }
            return Ok(());
        }

        match staged_commits.get(&entry.tx_id) {
            Some(existing) if existing == entry => Ok(()),
            Some(_) => Err(conflicting_terminal_record()),
            None => {
                staged_commits.insert(entry.tx_id, entry.clone());
                Ok(())
            }
        }
    }

    fn validate_replay_rollback_candidate(
        &self,
        entry: &RollbackLogEntry,
        staged_commits: &BTreeMap<TransactionId, CommitLogEntry>,
        staged_rollbacks: &mut BTreeMap<TransactionId, RollbackLogEntry>,
    ) -> AndromedaResult<()> {
        validate_transaction_id(entry.tx_id, "cannot replay rollback entry with zero ID")?;
        entry.validate_durable_evidence()?;
        if self.commit_entries.contains_key(&entry.tx_id)
            || staged_commits.contains_key(&entry.tx_id)
        {
            return Err(conflicting_terminal_record());
        }
        validate_terminal_status_compatible(
            &self.status_table,
            entry.tx_id,
            TransactionStatus::RolledBack,
        )?;

        if let Some(existing) = self.rollback_entries.get(&entry.tx_id) {
            if *existing != *entry {
                return Err(conflicting_terminal_record());
            }
            return Ok(());
        }

        match staged_rollbacks.get(&entry.tx_id) {
            Some(existing) if existing == entry => Ok(()),
            Some(_) => Err(conflicting_terminal_record()),
            None => {
                staged_rollbacks.insert(entry.tx_id, entry.clone());
                Ok(())
            }
        }
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

fn validate_durable_flush_covers_record(
    durable_lsn: Lsn,
    record_lsn: Lsn,
    record_name: &'static str,
) -> AndromedaResult<()> {
    if durable_lsn < record_lsn {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            format!(
                "durable WAL flush ended before {record_name} LSN: durable={}, record={}",
                durable_lsn.get(),
                record_lsn.get()
            ),
        ));
    }
    Ok(())
}

fn validate_replay_lsn_progression(
    previous_record: &mut Option<TxWalReplayRecord>,
    record: &TxWalReplayRecord,
) -> AndromedaResult<()> {
    let record_lsn = record.replay_lsn();
    if let Some(previous_record) = previous_record.as_ref() {
        let previous_lsn = previous_record.replay_lsn();
        if record_lsn < previous_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                format!(
                    "transaction WAL replay LSN order regressed: previous={}, current={}",
                    previous_lsn.get(),
                    record_lsn.get()
                ),
            ));
        }

        if record_lsn == previous_lsn && record != previous_record {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                format!(
                    "transaction WAL replay LSN reused by distinct records: lsn={}",
                    record_lsn.get()
                ),
            ));
        }
    }

    *previous_record = Some(record.clone());
    Ok(())
}

fn validate_terminal_status_compatible(
    status_table: &TransactionStatusTable,
    tx_id: TransactionId,
    replay_status: TransactionStatus,
) -> AndromedaResult<()> {
    match status_table.status(tx_id) {
        Some(existing) if existing == replay_status => Ok(()),
        Some(TransactionStatus::InFlight) => Ok(()),
        Some(_) => Err(conflicting_terminal_record()),
        None => Ok(()),
    }
}

fn conflicting_terminal_record() -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Transaction,
        "conflicting transaction terminal durability records",
    )
}
