//! Transaction commit log facade.
//!
//! The commit log owns the transaction visibility boundary: commit and rollback
//! terminal states are written and flushed to WAL before the MVCC status table is
//! updated.

mod entry;
mod manager;
mod replay;
mod rollback;
mod status_rebuild;
mod wal;

pub use entry::{CommitLogEntry, IsolationLevel, WalRecordKind};
pub use manager::CommitLogManager;
pub use replay::{TxWalReplayAction, TxWalReplayRecord, TxWalReplaySummary};
pub use rollback::RollbackLogEntry;
pub use status_rebuild::TransactionStatusRebuild;
pub use wal::InvocationWal;

#[cfg(test)]
use crate::{Lsn, TransactionStatus, TransactionStatusTable};
#[cfg(test)]
use andromeda_core::{AndromedaErrorKind, AndromedaResult, EngineTimestamp, TransactionId};

#[cfg(test)]
mod tests;
