//! Storage compatibility surface for the file-backed WAL.
//!
//! File WAL byte-format types are owned by `andromeda_wal`; startup recovery
//! DTOs and report contracts are owned by `andromeda_recovery`.

pub use andromeda_recovery::{
    FileWalRecoveryBoundaryKind, FileWalRecoveryIgnoredTransaction,
    FileWalRecoveryIgnoredTransactionReason, FileWalRecoveryReplayRecord, FileWalRecoveryReportV0,
    FileWalStartupRecoveryV0, plan_file_wal_startup_recovery_v0, recover_from_file_wal,
    report_file_wal_recovery_v0,
};
pub use andromeda_wal::{
    FILE_WAL_HEADER_LEN, FILE_WAL_MAGIC, FILE_WAL_MONO_SEGMENT_ID, FileWal, FileWalDiskScan,
    FileWalHeader, scan_file_wal,
};
