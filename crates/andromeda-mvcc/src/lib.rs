#![forbid(unsafe_code)]

mod active_snapshot_registry;
pub mod gc;
mod snapshot;
mod status;
mod version;

pub use active_snapshot_registry::{ActiveSnapshotRegistry, GcError, SnapshotHandle};
pub use gc::mvcc_eligibility::{
    VersionEligibility, VersionEligibilityChecker, VersionEligibilityStats, VersionRecord,
};
pub use gc::reclamation::{
    ReclamationCommand, ReclamationEligibility, ReclamationMark, ReclamationMarkCandidate,
    ReclamationStats,
};
pub use gc::{
    GcEligibilityChecker, GcSchedulerExit, GcSchedulerExitReason, GcSchedulerHandle,
    GcSchedulerStats, GcSchedulerTask, GcStatSnapshot, GcStats, GcSummary,
    MIN_GC_SCHEDULER_INTERVAL, MvccGarbageCollector,
};
pub use snapshot::{MvccIsolationPolicy, Snapshot};
pub use status::{TransactionStatus, TransactionStatusTable};
pub use version::{MvccRowHeader, creator_is_visible, delete_is_visible};
