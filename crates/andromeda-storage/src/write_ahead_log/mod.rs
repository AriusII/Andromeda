//! Write-ahead log domain facade: records, transaction tracking, in-memory WAL,
//! file-backed WAL, codec, and segment value types.
//!
//! Canonical ownership lives in single-source modules:
//!
//! | Type / item                                  | Canonical module                            |
//! |----------------------------------------------|---------------------------------------------|
//! | `WalRecord`, `WalRecordHeader`, `WalRecordKind`, checksum/tag helpers | [`record`] |
//! | `InMemoryWal` and durable-LSN tracking       | [`manager`]                                 |
//! | Transaction classification helpers           | [`transaction`]                             |
//! | `WalSegment`, `WalSegmentDescriptor`         | [`crate::wal_segment`]                      |
//! | WAL frame codec, scanner, byte constants     | [`crate::wal_codec`]                        |
//! | `FileWal`, `FileWalHeader`, recovery report  | [`crate::file_wal`]                         |
//!
//! The submodules below are thin re-export facades for the cross-domain types
//! (segment, codec, file). They MUST NOT define types of their own. The legacy
//! [`crate::wal`] root facade is preserved for compatibility with older imports.
//!
//! Doctrine reminders enforced by the items re-exported here:
//! * `visible commit == durable WAL` — frames are flushed before commit
//!   acknowledgement.
//! * `RAM is never truth` — recovery rebuilds state from the on-disk WAL alone.
//! * No unsafe code, no ad-hoc SQL, no runtime JSON normative protocol, no
//!   gRPC/tonic transport.

pub mod codec;
pub mod file {
    //! Facade for the canonical [`crate::file_wal`] module.
    //!
    //! Do not define new types here; add them under `crate::file_wal` and
    //! re-export.
    pub use crate::{
        recover_from_file_wal, report_file_wal_recovery_v0, scan_file_wal, FileWal,
        FileWalDiskScan, FileWalHeader, FileWalRecoveryBoundaryKind,
        FileWalRecoveryIgnoredTransaction, FileWalRecoveryIgnoredTransactionReason,
        FileWalRecoveryReplayRecord, FileWalRecoveryReportV0, FILE_WAL_HEADER_LEN, FILE_WAL_MAGIC,
        FILE_WAL_MONO_SEGMENT_ID,
    };
}
pub mod manager;
pub mod record;
pub mod segment;
pub mod shipping;
pub mod transaction;

pub use codec::*;
pub use file::*;
pub use manager::*;
pub use record::*;
pub use segment::*;
pub use shipping::*;
pub use transaction::*;
