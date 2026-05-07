//! Physical file-backed WAL owner.
//!
//! This module owns the explicit FileWal header codec, file-backed append and
//! flush behavior, durable-prefix scan, and byte-contract constants. It does
//! not own storage recovery reports, startup policy, replay planning, manifests,
//! page integration, or durable visibility decisions.

use andromeda_core::{AndromedaError, AndromedaErrorKind};

mod format;
mod header;
mod scan;
mod wal;

pub use header::{FILE_WAL_HEADER_LEN, FILE_WAL_MAGIC, FILE_WAL_MONO_SEGMENT_ID, FileWalHeader};
pub use scan::{FileWalDiskScan, scan_file_wal};
pub use wal::FileWal;

fn io_error(action: &str, error: std::io::Error) -> AndromedaError {
    storage_error(format!("{action}: {error}"))
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
