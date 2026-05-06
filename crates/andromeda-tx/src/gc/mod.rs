//! MVCC Garbage Collection Engine
//!
//! This module implements the core garbage collection logic for MVCC versions.
//! A row version is eligible for reclamation when:
//!
//! 1. Its creator transaction has been durably committed (per V0 doctrine), AND
//! 2. Its `end_ts` is strictly less than the minimum visible timestamp (no active
//!    snapshot can see it), AND
//! 3. The version is closed (`end_ts != u64::MAX`)
//!
//! # Thread Safety
//!
//! All GC operations are thread-safe and make no unsafe assumptions.
//! - `MvccGarbageCollector` can be shared via Arc
//! - Stats updates use AtomicU64 with relaxed ordering
//! - Reads from `TransactionStatusTable` and `ActiveSnapshotRegistry` are atomic
//!
//! # Design Decisions
//!
//! - **No separate HeapTable scanning**: GC assumes a higher-level coordinator
//!   provides page iteration and calls `mark_version_reclaimed()` per version.
//! - **Status lookups are conservative**: Only `Committed` status qualifies a creator.
//!   `InFlight` or `RolledBack` versions follow different rules (see logic below).
//! - **Idempotent marking**: Calling `mark_version_reclaimed()` multiple times is safe.

mod collector;
mod stats;

pub mod eligibility;
pub mod mvcc_eligibility;
pub mod reclamation;
pub mod scheduler;

#[cfg(test)]
mod tests;

pub use collector::{GcSummary, MvccGarbageCollector};
pub use eligibility::GcEligibilityChecker;
pub use mvcc_eligibility::{
    VersionEligibility, VersionEligibilityChecker, VersionEligibilityStats, VersionRecord,
};
pub use reclamation::{
    ReclamationCommand, ReclamationEligibility, ReclamationMark, ReclamationMarkCandidate,
    ReclamationStats,
};
pub use scheduler::{
    GcSchedulerExit, GcSchedulerExitReason, GcSchedulerHandle, GcSchedulerStats, GcSchedulerTask,
    MIN_GC_SCHEDULER_INTERVAL,
};
pub use stats::{GcStatSnapshot, GcStats};
