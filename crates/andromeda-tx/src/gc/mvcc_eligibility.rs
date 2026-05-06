//! MVCC Version Eligibility Checker for Garbage Collection
//!
//! This module implements the `VersionEligibilityChecker` which determines whether
//! a row version is safe to reclaim by checking three independent criteria:
//! 1. Creator transaction is durably committed
//! 2. Version's end timestamp is invisible to all active snapshots
//! 3. Grace period has elapsed since version was marked
//!
//! # Extended Handling: Aborted and In-Flight Versions
//!
//! - **Aborted versions** (creator_tx status = Aborted):
//!   - Immediately eligible for reclamation (no grace period, no visibility check needed)
//!   - Aborted transactions are never visible to any snapshot
//!   - Called via `is_aborted_version_eligible()`
//!
//! - **In-flight versions** (creator_tx status = Active/Committing):
//!   - Never eligible (in-flight tx might fail, releasing versions)
//!   - Must not be reclaimed until creator tx completes
//!   - Called via `is_in_flight_version_safe()` (returns false if in-flight)
//!
//! # Correctness Invariant
//!
//! **No visible version is ever marked eligible.**
//!
//! Proof by contradiction:
//! - Assume a version V is both eligible and visible.
//! - If V is eligible, then by the eligibility check: `V.end_ts < min_visible_ts`
//! - `min_visible_ts` is the minimum timestamp of all active snapshots
//! - If V is visible to snapshot S, then: `S.begin_ts >= V.end_ts` (MVCC definition of visibility)
//! - Therefore: `S.begin_ts >= V.end_ts > min_visible_ts` contradicts `min_visible_ts = min(all S.begin_ts)`
//! - Hence, no visible version can be marked eligible. QED.
//!
//! **Secondary Invariant: In-flight and Aborted Protection**
//!
//! - No version created by an in-flight transaction is ever reclaimed
//! - Versions created by aborted transactions are safe to reclaim immediately
//!
//! # Trace Emission
//!
//! Each eligibility check emits a `VersionEligibilityCheckTrace` with:
//! - Version ID and metadata
//! - Eligibility decision (eligible, ineligible + reason)
//! - Criterion values (creator_committed, end_ts_invisible, grace_period_met)
//! - Additional fields for aborted/in-flight handling
//! - Timestamp when check occurred
//!
//! # Thread Safety
//!
//! All operations are thread-safe via Arc-wrapped shared references.
//! Status table and snapshot registry queries may block temporarily but
//! never deadlock due to careful ordering (no cyclic waiting).

mod checker;
mod types;

#[cfg(test)]
mod tests;

pub use checker::VersionEligibilityChecker;
pub use types::{
    Timestamp, VersionEligibility, VersionEligibilityCheckTrace, VersionEligibilityStats,
    VersionId, VersionRecord,
};
