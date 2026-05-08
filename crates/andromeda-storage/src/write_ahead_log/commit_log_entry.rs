//! Compatibility reexports for commit log WAL durability evidence.
//!
//! The canonical commit log cache and entry codec live in
//! `andromeda_wal::write_ahead_log::commit_log_entry`. Storage keeps this shim
//! so existing integration imports continue to resolve.

pub use andromeda_wal::{CommitLog, CommitLogEntry, Timestamp};
