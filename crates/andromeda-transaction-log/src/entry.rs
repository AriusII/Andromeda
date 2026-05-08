use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, EngineTimestamp, TransactionId,
};

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
#[non_exhaustive]
pub struct CommitLogEntry {
    pub tx_id: TransactionId,
    pub commit_lsn: Lsn,
    pub durable_lsn: Lsn,
    pub timestamp: EngineTimestamp,
    pub row_count_affected: u64,
    pub isolation_level: IsolationLevel,
}

impl CommitLogEntry {
    pub fn from_durable_wal(
        tx_id: TransactionId,
        commit_lsn: Lsn,
        durable_lsn: Lsn,
        timestamp: EngineTimestamp,
        row_count_affected: u64,
        isolation_level: IsolationLevel,
    ) -> AndromedaResult<Self> {
        let entry = Self {
            tx_id,
            commit_lsn,
            durable_lsn,
            timestamp,
            row_count_affected,
            isolation_level,
        };
        entry.validate_durable_evidence()?;
        Ok(entry)
    }

    pub fn is_before(&self, threshold_lsn: Lsn) -> bool {
        self.commit_lsn < threshold_lsn
    }

    pub fn validate_durable_evidence(&self) -> AndromedaResult<()> {
        if self.tx_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "commit log entry transaction id must not be zero",
            ));
        }

        if self.commit_lsn.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "commit log entry commit LSN must not be zero",
            ));
        }

        if self.durable_lsn < self.commit_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!(
                    "durable WAL flush ended before commit LSN: durable={}, commit={}",
                    self.durable_lsn.get(),
                    self.commit_lsn.get()
                ),
            ));
        }

        Ok(())
    }
}
