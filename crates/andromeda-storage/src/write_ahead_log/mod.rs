//! Write-ahead log domain facade: records, transaction tracking, in-memory WAL,
//! file-backed WAL, codec, segment value types, garbage collection, and compaction.
//!
//! Canonical ownership lives in single-source modules:
//!
//! | Type / item                                  | Canonical module                            |
//! |----------------------------------------------|---------------------------------------------|
//! | `WalRecord`, `WalRecordHeader`, `WalRecordKind`, checksum/tag helpers | `andromeda_wal::write_ahead_log::record` |
//! | `InMemoryWal` and durable-LSN tracking       | `andromeda_wal::write_ahead_log::manager`   |
//! | Transaction classification helpers           | `andromeda_wal::write_ahead_log::transaction` |
//! | `WalSegment`, `WalSegmentDescriptor`         | `andromeda_wal::wal_segment`                |
//! | WAL frame codec, scanner, byte constants     | `andromeda_wal::wal_codec`                  |
//! | `FileWal`, `FileWalHeader`, recovery report  | `crate::file_wal`                           |
//! | WAL GC: candidates, archive verification    | [`gc`]                                      |
//! | WAL Compaction: fragmentation, scheduling   | [`compaction`]                              |
//! | CommitLogEntry and CommitLog persistence    | [`commit_log_entry`]                        |
//!
//! The pure WAL submodules below are thin re-export facades for `andromeda_wal`.
//! They MUST NOT define types of their own. The legacy `crate::wal` root facade
//! is preserved for compatibility with older imports.
//!
//! Doctrine reminders enforced by the items re-exported here:
//! * `visible commit == durable WAL` — frames are flushed before commit
//!   acknowledgement.
//! * `RAM is never truth` — recovery rebuilds state from the on-disk WAL alone.
//! * No unsafe code, no ad-hoc SQL, no runtime JSON normative protocol, no
//!   gRPC/tonic transport.

pub mod codec;
pub mod commit_log_entry;
pub mod commit_log_facade;
pub mod compaction;
pub mod durability_fence;
pub mod file {
    //! Facade for the canonical `crate::file_wal` module.
    //!
    //! Do not define new types here; add them under `crate::file_wal` and
    //! re-export.
    pub use crate::{
        FILE_WAL_HEADER_LEN, FILE_WAL_MAGIC, FILE_WAL_MONO_SEGMENT_ID, FileWal, FileWalDiskScan,
        FileWalHeader, FileWalRecoveryBoundaryKind, FileWalRecoveryIgnoredTransaction,
        FileWalRecoveryIgnoredTransactionReason, FileWalRecoveryReplayRecord,
        FileWalRecoveryReportV0, recover_from_file_wal, report_file_wal_recovery_v0, scan_file_wal,
    };
}
pub mod gc;
pub mod gc_eligibility;
pub mod heap_redo;
pub mod manager;
pub mod record;
pub mod record_bounds;
pub mod segment;
pub mod segment_reclaimability;
pub mod shipping;
pub mod transaction;

pub use codec::*;
pub use commit_log_entry::*;
pub use commit_log_facade::*;
pub use compaction::*;
pub use durability_fence::*;
pub use file::*;
pub use gc::*;
pub use gc_eligibility::*;
pub use heap_redo::*;
pub use manager::*;
pub use record::*;
pub use record_bounds::*;
pub use segment::*;
pub use segment_reclaimability::*;
pub use shipping::*;
pub use transaction::*;
