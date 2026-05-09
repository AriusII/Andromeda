//! Commit log entry model and WAL durability evidence cache.
//!
//! The commit log records assigned WAL commit LSNs and defers visibility until
//! the corresponding WAL flush is confirmed. It is a reconstructable cache; the
//! WAL remains the durable source of truth.

mod entry;
mod store;

use andromeda_error::{AndromedaError, AndromedaErrorKind};

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
