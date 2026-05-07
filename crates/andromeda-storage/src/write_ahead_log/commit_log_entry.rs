//! Commit Log Entry Model and Persistence Layer
//!
//! This module defines the type system for commit log entries and their lifecycle,
//! linking transaction commits to WAL durability evidence via LSN tracking.
//!
//! # Durability Invariants
//!
//! **Invariant 1: Commit log entry created for an assigned WAL commit LSN**
//! - Entry cannot exist before the caller has assigned the corresponding WAL commit LSN
//! - The caller owns the WAL append boundary; this type records the durable-ordering evidence
//!
//! **Invariant 2: Durable flag set AFTER WAL flush confirmation**
//! - Flag must remain `false` until WAL flush completes
//! - Flipping from `false` → `true` is atomic operation
//! - Never flipped back to `false`
//!
//! **Invariant 3: Never mark visible until durable flag is true**
//! - TransactionStatusTable update deferred until `is_durable() == true`
//! - Ensures "visible commit ≡ durable WAL" doctrine
//!
//! **Invariant 4: GC Eligibility based on LSN**
//! - Entries with `commit_lsn < min_active_snapshot_lsn` are GC candidates
//! - Cleanup only after durability confirmation and visibility update
//!
//! # Thread Safety
//!
//! `CommitLog` uses DashMap for concurrent access without locking entire table.
//! Per-entry durability transitions are made through DashMap entry mutation.
//!
//! # Encoding Format
//!
//! Binary format for persistence (25 bytes total, little-endian integer fields):
//! - Bytes 0-7:   `tx_id` (u64)
//! - Bytes 8-15:  `commit_lsn` (u64)
//! - Bytes 16-23: `visible_timestamp` (u64)
//! - Byte 24:     `wal_durability_confirmed` (u8: 0 or 1)

mod entry;
mod store;

use andromeda_core::{AndromedaError, AndromedaErrorKind};

pub use entry::{CommitLogEntry, Timestamp};
pub use store::CommitLog;

pub(crate) fn storage_error(msg: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, msg)
}

pub(crate) fn transaction_error(msg: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Transaction, msg)
}

#[cfg(test)]
use entry::{
    COMMIT_LOG_ENTRY_ENCODED_LEN, DURABILITY_FLAG_OFFSET, WAL_DURABILITY_CONFIRMED,
    WAL_DURABILITY_UNCONFIRMED,
};

#[cfg(test)]
mod tests;
