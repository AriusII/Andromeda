//! Storage-owned WAL integration surface.
//!
//! Doctrine reminders enforced by the items re-exported here:
//! * `visible commit == durable WAL` — frames are flushed before commit
//!   acknowledgement.
//! * `RAM is never truth` — recovery rebuilds state from the on-disk WAL alone.
//! * No unsafe code, no ad-hoc SQL, no runtime JSON normative protocol, no
//!   gRPC/tonic transport.

pub mod durability_fence;
pub mod file {
    //! Storage recovery projection for file-backed WAL scans.

    pub use crate::{
        FileWalRecoveryBoundaryKind, FileWalRecoveryIgnoredTransaction,
        FileWalRecoveryIgnoredTransactionReason, FileWalRecoveryReplayRecord,
        FileWalRecoveryReportV0, FileWalStartupRecoveryV0, plan_file_wal_startup_recovery_v0,
        recover_from_file_wal, report_file_wal_recovery_v0,
    };
}

pub use durability_fence::*;
pub use file::*;
