use andromeda_core::{EngineTimestamp, TransactionId};

use crate::Lsn;

use super::entry::{CommitLogEntry, IsolationLevel};
use super::rollback::RollbackLogEntry;

/// Transaction-local WAL replay record used by recovery code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TxWalReplayRecord {
    Commit(CommitLogEntry),
    Rollback(RollbackLogEntry),
    Incomplete { tx_id: TransactionId, last_lsn: Lsn },
}

impl TxWalReplayRecord {
    pub fn commit(
        tx_id: TransactionId,
        commit_lsn: Lsn,
        timestamp: EngineTimestamp,
        row_count_affected: u64,
        isolation_level: IsolationLevel,
    ) -> Self {
        Self::commit_with_durable_lsn(
            tx_id,
            commit_lsn,
            commit_lsn,
            timestamp,
            row_count_affected,
            isolation_level,
        )
    }

    pub fn commit_with_durable_lsn(
        tx_id: TransactionId,
        commit_lsn: Lsn,
        durable_lsn: Lsn,
        timestamp: EngineTimestamp,
        row_count_affected: u64,
        isolation_level: IsolationLevel,
    ) -> Self {
        Self::Commit(CommitLogEntry {
            tx_id,
            commit_lsn,
            durable_lsn,
            timestamp,
            row_count_affected,
            isolation_level,
        })
    }

    pub fn rollback(
        tx_id: TransactionId,
        rollback_lsn: Lsn,
        timestamp: EngineTimestamp,
        parameter_hash: u64,
    ) -> Self {
        Self::rollback_with_durable_lsn(
            tx_id,
            rollback_lsn,
            rollback_lsn,
            timestamp,
            parameter_hash,
        )
    }

    pub fn rollback_with_durable_lsn(
        tx_id: TransactionId,
        rollback_lsn: Lsn,
        durable_lsn: Lsn,
        timestamp: EngineTimestamp,
        parameter_hash: u64,
    ) -> Self {
        Self::Rollback(RollbackLogEntry {
            tx_id,
            rollback_lsn,
            durable_lsn,
            timestamp,
            parameter_hash,
        })
    }

    pub fn incomplete(tx_id: TransactionId, last_lsn: Lsn) -> Self {
        Self::Incomplete { tx_id, last_lsn }
    }

    pub fn tx_id(&self) -> TransactionId {
        match self {
            Self::Commit(entry) => entry.tx_id,
            Self::Rollback(entry) => entry.tx_id,
            Self::Incomplete { tx_id, .. } => *tx_id,
        }
    }

    pub fn replay_lsn(&self) -> Lsn {
        match self {
            Self::Commit(entry) => entry.commit_lsn,
            Self::Rollback(entry) => entry.rollback_lsn,
            Self::Incomplete { last_lsn, .. } => *last_lsn,
        }
    }
}

/// Per-record action taken by transaction WAL replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxWalReplayAction {
    CommitRestored,
    RollbackRestored,
    DuplicateCommit,
    DuplicateRollback,
    IncompleteIgnored,
}

/// Summary returned after replaying transaction-local WAL records.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TxWalReplaySummary {
    pub commits_restored: usize,
    pub rollbacks_restored: usize,
    pub duplicate_commits: usize,
    pub duplicate_rollbacks: usize,
    pub incomplete_transactions: usize,
}

impl TxWalReplaySummary {
    pub(super) fn record(&mut self, action: TxWalReplayAction) {
        match action {
            TxWalReplayAction::CommitRestored => self.commits_restored += 1,
            TxWalReplayAction::RollbackRestored => self.rollbacks_restored += 1,
            TxWalReplayAction::DuplicateCommit => self.duplicate_commits += 1,
            TxWalReplayAction::DuplicateRollback => self.duplicate_rollbacks += 1,
            TxWalReplayAction::IncompleteIgnored => self.incomplete_transactions += 1,
        }
    }
}
