//! Write-ahead log domain: records, transaction tracking, and in-memory WAL.
//!
//! The WAL record model, byte codec, and segment descriptor stay available from
//! the crate root for compatibility. This module provides a coherent domain
//! hierarchy for new code and external integration tests.

pub mod codec;
pub mod file {
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
pub mod transaction;

pub use codec::*;
pub use file::*;
pub use manager::*;
pub use record::*;
pub use segment::*;
pub use transaction::*;
