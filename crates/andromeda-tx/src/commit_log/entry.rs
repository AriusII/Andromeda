use andromeda_core::{EngineTimestamp, TransactionId};

use crate::Lsn;

/// Transaction WAL record kinds required by the commit boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalRecordKind {
    TxCommit,
    TxRollback,
}

/// Isolation level for transaction commit classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationLevel {
    Snapshot,
    Serializable,
}

/// Durable commit entry linking a transaction to its commit LSN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitLogEntry {
    pub tx_id: TransactionId,
    pub commit_lsn: Lsn,
    pub timestamp: EngineTimestamp,
    pub row_count_affected: u64,
    pub isolation_level: IsolationLevel,
}

impl CommitLogEntry {
    pub fn is_before(&self, threshold_lsn: Lsn) -> bool {
        self.commit_lsn < threshold_lsn
    }
}
