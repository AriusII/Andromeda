use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, EngineTimestamp, TransactionId,
};

use crate::Lsn;

/// Durable rollback entry linking a transaction to its rollback LSN.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RollbackLogEntry {
    pub tx_id: TransactionId,
    pub rollback_lsn: Lsn,
    pub durable_lsn: Lsn,
    pub timestamp: EngineTimestamp,
    pub parameter_hash: u64,
}

impl RollbackLogEntry {
    pub fn from_durable_wal(
        tx_id: TransactionId,
        rollback_lsn: Lsn,
        durable_lsn: Lsn,
        timestamp: EngineTimestamp,
        parameter_hash: u64,
    ) -> AndromedaResult<Self> {
        let entry = Self {
            tx_id,
            rollback_lsn,
            durable_lsn,
            timestamp,
            parameter_hash,
        };
        entry.validate_durable_evidence()?;
        Ok(entry)
    }

    pub fn validate_durable_evidence(&self) -> AndromedaResult<()> {
        if self.tx_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "rollback log entry transaction id must not be zero",
            ));
        }

        if self.rollback_lsn.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "rollback log entry rollback LSN must not be zero",
            ));
        }

        if self.durable_lsn < self.rollback_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!(
                    "durable WAL flush ended before rollback LSN: durable={}, rollback={}",
                    self.durable_lsn.get(),
                    self.rollback_lsn.get()
                ),
            ));
        }

        Ok(())
    }
}
