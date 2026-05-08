use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use std::collections::BTreeMap;

use crate::Lsn;
use crate::mvcc_status::TransactionStatus;

use super::super::entry::CommitLogEntry;
use super::super::replay::{TxWalReplayAction, TxWalReplayRecord, TxWalReplaySummary};
use super::super::rollback::RollbackLogEntry;
use super::super::status_rebuild::TransactionStatusRebuild;
use super::CommitLogManager;
use super::validation::{
    conflicting_terminal_record, validate_replay_lsn_progression,
    validate_terminal_status_compatible, validate_transaction_id,
};

impl CommitLogManager {
    pub fn replay_tx_wal_record(
        &self,
        record: TxWalReplayRecord,
    ) -> AndromedaResult<TxWalReplayAction> {
        match record {
            TxWalReplayRecord::Commit(entry) => self.replay_commit_record(entry),
            TxWalReplayRecord::Rollback(entry) => self.replay_rollback_record(entry),
            TxWalReplayRecord::Incomplete { tx_id, last_lsn } => {
                validate_incomplete_replay_record(tx_id, last_lsn)?;
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
                    validate_incomplete_replay_record(*tx_id, *last_lsn)?;
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
}

fn validate_incomplete_replay_record(tx_id: TransactionId, last_lsn: Lsn) -> AndromedaResult<()> {
    validate_transaction_id(tx_id, "cannot replay incomplete transaction with zero ID")?;
    if last_lsn.get() == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            "cannot replay incomplete transaction with zero LSN",
        ));
    }
    Ok(())
}
