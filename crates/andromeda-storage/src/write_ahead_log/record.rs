//! Compatibility reexports for pure WAL record types.
//!
//! The canonical definitions moved to `andromeda_wal::write_ahead_log::record`.
//! Keep this module as a stable storage import path until downstream callers
//! migrate to the WAL crate directly.

pub use andromeda_wal::write_ahead_log::record::{
    WalRecord, WalRecordHeader, WalRecordKind, wal_record_checksum, wal_record_kind_from_tag,
    wal_record_kind_tag,
};
