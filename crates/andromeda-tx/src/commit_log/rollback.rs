use andromeda_core::{EngineTimestamp, TransactionId};

use crate::Lsn;

/// Durable rollback entry linking a transaction to its rollback LSN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackLogEntry {
    pub tx_id: TransactionId,
    pub rollback_lsn: Lsn,
    pub timestamp: EngineTimestamp,
    pub parameter_hash: u64,
}
