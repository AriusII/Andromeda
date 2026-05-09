//! Transaction commit-log manager.
//!
//! The durable record contracts live in `andromeda-transaction-log`. This
//! module owns the live append-flush-publish coordinator that mirrors durable
//! terminal evidence into the MVCC status table.

mod manager;

pub use andromeda_mvcc::{TransactionStatus, TransactionStatusTable};
pub use andromeda_transaction_log::{
    CommitLogEntry, InvocationWal, IsolationLevel, Lsn, RollbackLogEntry, TransactionStatusRebuild,
    TxWalReplayAction, TxWalReplayRecord, TxWalReplaySummary, WalRecordKind,
};
pub use manager::CommitLogManager;

#[cfg(test)]
use andromeda_error::{AndromedaErrorKind, AndromedaResult};
#[cfg(test)]
use andromeda_time::EngineTimestamp;
#[cfg(test)]
use andromeda_types::TransactionId;

#[cfg(test)]
mod tests;
