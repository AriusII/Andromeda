//! Compatibility re-exports for MVCC garbage collection.
//!
//! The implementation lives in `andromeda-mvcc`; `andromeda-tx` keeps this
//! module as the legacy import path while MVCC ownership is extracted.

pub mod eligibility {
    pub use andromeda_mvcc::gc::eligibility::*;
}

pub mod mvcc_eligibility {
    pub use andromeda_mvcc::gc::mvcc_eligibility::*;
}

pub mod reclamation {
    pub use andromeda_mvcc::gc::reclamation::*;
}

pub mod scheduler {
    pub use andromeda_mvcc::gc::scheduler::*;
}

pub use andromeda_mvcc::gc::{
    GcEligibilityChecker, GcSchedulerExit, GcSchedulerExitReason, GcSchedulerHandle,
    GcSchedulerStats, GcSchedulerTask, GcStatSnapshot, GcStats, GcSummary,
    MIN_GC_SCHEDULER_INTERVAL, MvccGarbageCollector,
};
pub use mvcc_eligibility::{
    VersionEligibility, VersionEligibilityChecker, VersionEligibilityStats, VersionRecord,
};
pub use reclamation::{
    ReclamationCommand, ReclamationEligibility, ReclamationMark, ReclamationMarkCandidate,
    ReclamationStats,
};
