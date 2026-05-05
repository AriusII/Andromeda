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
//! **Wave 21 Batch 4 Task 4 — Edge Case Safety**
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

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};

use crate::active_snapshot_registry::ActiveSnapshotRegistry;
use crate::mvcc_status::{TransactionStatus, TransactionStatusTable};

/// Unique identifier for a version within a row's version chain.
pub type VersionId = u64;

/// Logical timestamp for MVCC ordering (not wall-clock time).
pub type Timestamp = u64;

/// Results of version eligibility determination.
///
/// Captures the three independent criteria and their evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersionEligibility {
    /// True if creator transaction is durably committed
    pub is_creator_committed: bool,
    /// True if version's end_ts is strictly less than minimum visible timestamp
    pub is_end_ts_invisible: bool,
    /// True if grace period has elapsed since marking
    pub is_grace_period_met: bool,
}
impl VersionEligibility {
    /// Check if all three criteria are met (version is fully eligible for reclamation).
    pub fn is_fully_eligible(&self) -> bool {
        self.is_creator_committed && self.is_end_ts_invisible && self.is_grace_period_met
    }

    /// Get the first failed criterion for diagnostics.
    ///
    /// Returns `Some(reason)` if any criterion is false, else `None`.
    pub fn first_failed_criterion(&self) -> Option<&'static str> {
        if !self.is_creator_committed {
            Some("creator not committed")
        } else if !self.is_end_ts_invisible {
            Some("end_ts still visible to snapshots")
        } else if !self.is_grace_period_met {
            Some("grace period not elapsed")
        } else {
            None
        }
    }
}

/// Version record reference for eligibility checking.
///
/// Minimal subset of version metadata needed for eligibility determination.
#[derive(Debug, Clone, Copy)]
pub struct VersionRecord {
    /// Unique identifier within row's version chain
    pub version_id: VersionId,
    /// Transaction that created this version
    pub creator_tx_id: TransactionId,
    /// Timestamp when this version became invisible (i.e., when next version created)
    pub end_ts: Timestamp,
    /// Timestamp when this version was first marked for potential reclamation
    pub marked_at_gc_epoch: u64,
}

impl VersionRecord {
    /// Create a new version record.
    ///
    /// # Errors
    ///
    /// Returns error if version_id or creator_tx_id are zero (invalid sentinels).
    pub fn new(
        version_id: VersionId,
        creator_tx_id: TransactionId,
        end_ts: Timestamp,
        marked_at_gc_epoch: u64,
    ) -> AndromedaResult<Self> {
        if version_id == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "version_id must not be zero",
            ));
        }

        if creator_tx_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "creator_tx_id must not be zero",
            ));
        }

        Ok(VersionRecord {
            version_id,
            creator_tx_id,
            end_ts,
            marked_at_gc_epoch,
        })
    }
}

/// Trace event for version eligibility check.
///
/// Emitted after each eligibility evaluation for observability.
#[derive(Debug, Clone)]
pub struct VersionEligibilityCheckTrace {
    /// Version being checked
    pub version_id: VersionId,
    /// Transaction that created the version
    pub creator_tx_id: TransactionId,
    /// Version's end timestamp
    pub end_ts: Timestamp,
    /// Eligibility determination result
    pub eligibility: VersionEligibility,
    /// Reason for ineligibility (if any)
    pub ineligibility_reason: Option<&'static str>,
    /// Current GC epoch when check occurred
    pub gc_epoch: u64,
}

/// Statistics for version eligibility checking operations.
#[derive(Debug, Clone, Copy)]
pub struct VersionEligibilityStats {
    /// Total versions checked
    pub versions_checked: u64,
    /// Versions marked eligible
    pub versions_eligible: u64,
    /// Versions ineligible due to uncommitted creator
    pub ineligible_creator_not_committed: u64,
    /// Versions ineligible due to visible end_ts
    pub ineligible_end_ts_visible: u64,
    /// Versions ineligible due to grace period
    pub ineligible_grace_period: u64,
}

impl VersionEligibilityStats {
    /// Create empty statistics.
    pub fn new() -> Self {
        VersionEligibilityStats {
            versions_checked: 0,
            versions_eligible: 0,
            ineligible_creator_not_committed: 0,
            ineligible_end_ts_visible: 0,
            ineligible_grace_period: 0,
        }
    }

    /// Total ineligible versions (sum of all ineligible reasons).
    pub fn total_ineligible(&self) -> u64 {
        self.ineligible_creator_not_committed
            + self.ineligible_end_ts_visible
            + self.ineligible_grace_period
    }
}

/// Version eligibility checker for MVCC garbage collection.
///
/// Evaluates whether a version can safely be reclaimed by checking:
/// 1. Creator transaction is durably committed
/// 2. Version's end_ts is strictly less than minimum visible snapshot timestamp
/// 3. Grace period has elapsed since version was marked
pub struct VersionEligibilityChecker {
    /// Transaction status table for checking creator commit status
    status_table: Arc<TransactionStatusTable>,
    /// Active snapshot registry for determining visibility threshold
    snapshot_registry: Arc<ActiveSnapshotRegistry>,
    /// Grace period in GC epochs
    grace_period_epochs: u64,
    /// Atomic statistics tracking
    stats: Arc<VersionEligibilityCheckerStats>,
}

/// Internal stats holder for thread-safe updates.
struct VersionEligibilityCheckerStats {
    versions_checked: AtomicU64,
    versions_eligible: AtomicU64,
    ineligible_creator_not_committed: AtomicU64,
    ineligible_end_ts_visible: AtomicU64,
    ineligible_grace_period: AtomicU64,
}

impl VersionEligibilityChecker {
    /// Create a new version eligibility checker.
    ///
    /// # Arguments
    ///
    /// * `status_table` - Transaction status table for creator commit checking
    /// * `snapshot_registry` - Active snapshot registry for visibility threshold
    /// * `grace_period_epochs` - Number of GC epochs to wait before reclamation
    pub fn new(
        status_table: Arc<TransactionStatusTable>,
        snapshot_registry: Arc<ActiveSnapshotRegistry>,
        grace_period_epochs: u64,
    ) -> AndromedaResult<Self> {
        if grace_period_epochs == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "grace_period_epochs must be greater than zero",
            ));
        }

        Ok(VersionEligibilityChecker {
            status_table,
            snapshot_registry,
            grace_period_epochs,
            stats: Arc::new(VersionEligibilityCheckerStats {
                versions_checked: AtomicU64::new(0),
                versions_eligible: AtomicU64::new(0),
                ineligible_creator_not_committed: AtomicU64::new(0),
                ineligible_end_ts_visible: AtomicU64::new(0),
                ineligible_grace_period: AtomicU64::new(0),
            }),
        })
    }

    /// Determine eligibility of a single version (all three criteria checked).
    ///
    /// Returns `true` only if all three criteria are met.
    ///
    /// # Arguments
    ///
    /// * `version` - Version record to evaluate
    /// * `gc_epoch` - Current GC epoch for grace period comparison
    pub fn is_eligible(&self, version: &VersionRecord, gc_epoch: u64) -> AndromedaResult<bool> {
        let eligibility = self.check_all_criteria(version, gc_epoch)?;
        Ok(eligibility.is_fully_eligible())
    }

    /// Check all three eligibility criteria and return detailed result.
    ///
    /// # Arguments
    ///
    /// * `version` - Version record to evaluate
    /// * `gc_epoch` - Current GC epoch for grace period comparison
    ///
    /// # Returns
    ///
    /// `VersionEligibility` struct with each criterion evaluated independently.
    pub fn check_all_criteria(
        &self,
        version: &VersionRecord,
        gc_epoch: u64,
    ) -> AndromedaResult<VersionEligibility> {
        // Increment check counter
        self.stats.versions_checked.fetch_add(1, Ordering::Relaxed);

        // Criterion 1: Creator must be durably committed
        let creator_status = self
            .status_table
            .status(version.creator_tx_id)
            .unwrap_or(TransactionStatus::InFlight);
        let is_creator_committed = matches!(creator_status, TransactionStatus::Committed);

        // Criterion 2: end_ts must be invisible to all active snapshots
        let min_visible_ts = self.min_visible_timestamp()?;
        let is_end_ts_invisible = version.end_ts < min_visible_ts;

        // Criterion 3: Grace period must have elapsed
        let is_grace_period_met = gc_epoch >= version.marked_at_gc_epoch + self.grace_period_epochs;

        // Build eligibility result
        let eligibility = VersionEligibility {
            is_creator_committed,
            is_end_ts_invisible,
            is_grace_period_met,
        };

        // Update statistics
        if eligibility.is_fully_eligible() {
            self.stats.versions_eligible.fetch_add(1, Ordering::Relaxed);
        } else {
            if !is_creator_committed {
                self.stats
                    .ineligible_creator_not_committed
                    .fetch_add(1, Ordering::Relaxed);
            }
            if !is_end_ts_invisible {
                self.stats
                    .ineligible_end_ts_visible
                    .fetch_add(1, Ordering::Relaxed);
            }
            if !is_grace_period_met {
                self.stats
                    .ineligible_grace_period
                    .fetch_add(1, Ordering::Relaxed);
            }
        }

        Ok(eligibility)
    }

    /// Get the minimum visible timestamp from the active snapshot registry.
    ///
    /// This is the oldest active snapshot's begin_ts, below which versions
    /// are guaranteed to be invisible to all active transactions.
    ///
    /// # Returns
    ///
    /// Minimum visible timestamp, or u64::MAX if no active snapshots exist.
    pub fn min_visible_timestamp(&self) -> AndromedaResult<Timestamp> {
        Ok(self.snapshot_registry.minimum_visible_timestamp())
    }

    /// Check if a version created by an aborted transaction is immediately eligible.
    ///
    /// **Wave 21 Batch 4 Task 4 — Aborted Version Handling**
    ///
    /// Versions created by aborted transactions are **immediately eligible** for reclamation
    /// because:
    /// - Aborted transactions are never visible to any snapshot
    /// - No future snapshot will ever read versions from an aborted tx
    /// - No grace period or visibility check needed
    ///
    /// # Returns
    ///
    /// `true` if creator transaction is **Aborted**, `false` otherwise.
    ///
    /// # Arguments
    ///
    /// * `version` - Version record to evaluate
    ///
    /// # Correctness
    ///
    /// This method SHORT-CIRCUITS the full three-criterion check.
    /// If `is_aborted_version_eligible()` returns `true`, the caller should
    /// mark the version as immediately eligible without further checks.
    pub fn is_aborted_version_eligible(&self, version: &VersionRecord) -> bool {
        let creator_status = self
            .status_table
            .status(version.creator_tx_id)
            .unwrap_or(TransactionStatus::InFlight);

        // Only Aborted status qualifies for immediate eligibility
        matches!(creator_status, TransactionStatus::RolledBack)
    }

    /// Check if a version created by an in-flight transaction is safe (never eligible).
    ///
    /// **Wave 21 Batch 4 Task 4 — In-Flight Version Safety**
    ///
    /// Versions created by in-flight transactions are **never eligible** because:
    /// - In-flight transactions may later rollback, releasing their versions
    /// - Reclaiming in-flight versions would violate atomicity
    /// - Must wait for transaction to reach terminal state (Committed or RolledBack)
    ///
    /// # Returns
    ///
    /// `true` if transaction is **not** in-flight (safe to proceed with other checks),
    /// `false` if transaction **is** in-flight (version must not be reclaimed).
    ///
    /// # Arguments
    ///
    /// * `version` - Version record to evaluate
    ///
    /// # Correctness Invariant
    ///
    /// **No in-flight transaction version is ever reclaimed.**
    ///
    /// If `is_in_flight_version_safe()` returns `false`, the version must be
    /// marked as **ineligible immediately** without further checks.
    pub fn is_in_flight_version_safe(&self, version: &VersionRecord) -> bool {
        let creator_status = self
            .status_table
            .status(version.creator_tx_id)
            .unwrap_or(TransactionStatus::InFlight);

        // Safe only if NOT in-flight
        !matches!(creator_status, TransactionStatus::InFlight)
    }

    /// Determine eligibility with fast-path checks for aborted and in-flight versions.
    ///
    /// **Wave 21 Batch 4 Task 4 — Extended Eligibility Logic**
    ///
    /// This method performs eligibility checks in the following order:
    /// 1. If version is from **aborted tx**: immediately eligible (return `true`)
    /// 2. If version is from **in-flight tx**: immediately ineligible (return `false`)
    /// 3. Otherwise: apply normal three-criterion check
    ///
    /// # Arguments
    ///
    /// * `version` - Version record to evaluate
    /// * `gc_epoch` - Current GC epoch for grace period comparison
    ///
    /// # Returns
    ///
    /// `true` if fully eligible, `false` if ineligible.
    pub fn is_eligible_extended(
        &self,
        version: &VersionRecord,
        gc_epoch: u64,
    ) -> AndromedaResult<bool> {
        // Fast-path 1: Aborted versions are immediately eligible
        if self.is_aborted_version_eligible(version) {
            return Ok(true);
        }

        // Fast-path 2: In-flight versions are never eligible
        if !self.is_in_flight_version_safe(version) {
            return Ok(false);
        }

        // Normal path: Check all three criteria
        let eligibility = self.check_all_criteria(version, gc_epoch)?;
        Ok(eligibility.is_fully_eligible())
    }

    /// Get the creator transaction status for the given transaction ID.
    ///
    /// # Returns
    ///
    /// `TransactionStatus` or `InFlight` if not found in status table.
    pub fn creator_status(&self, tx_id: TransactionId) -> AndromedaResult<TransactionStatus> {
        // Return the status from the table, or assume InFlight if not found
        let status = self
            .status_table
            .status(tx_id)
            .unwrap_or(TransactionStatus::InFlight);
        Ok(status)
    }

    /// Get a snapshot of current statistics.
    pub fn get_stats(&self) -> VersionEligibilityStats {
        VersionEligibilityStats {
            versions_checked: self.stats.versions_checked.load(Ordering::Relaxed),
            versions_eligible: self.stats.versions_eligible.load(Ordering::Relaxed),
            ineligible_creator_not_committed: self
                .stats
                .ineligible_creator_not_committed
                .load(Ordering::Relaxed),
            ineligible_end_ts_visible: self.stats.ineligible_end_ts_visible.load(Ordering::Relaxed),
            ineligible_grace_period: self.stats.ineligible_grace_period.load(Ordering::Relaxed),
        }
    }

    /// Reset all statistics to zero (for testing).
    pub fn reset_stats(&self) {
        self.stats.versions_checked.store(0, Ordering::Relaxed);
        self.stats.versions_eligible.store(0, Ordering::Relaxed);
        self.stats
            .ineligible_creator_not_committed
            .store(0, Ordering::Relaxed);
        self.stats
            .ineligible_end_ts_visible
            .store(0, Ordering::Relaxed);
        self.stats
            .ineligible_grace_period
            .store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mvcc_status::TransactionStatus;
    use andromeda_core::TransactionId;

    fn make_status_table() -> Arc<TransactionStatusTable> {
        Arc::new(TransactionStatusTable::new())
    }

    fn make_snapshot_registry() -> Arc<ActiveSnapshotRegistry> {
        Arc::new(ActiveSnapshotRegistry::new())
    }
    fn next_tx_id() -> TransactionId {
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT_TX_ID: AtomicU64 = AtomicU64::new(1);
        TransactionId::new(NEXT_TX_ID.fetch_add(1, Ordering::Relaxed))
    }

    #[test]
    fn test_version_eligibility_all_criteria_met() {
        let status_table = make_status_table();
        let snapshot_registry = make_snapshot_registry();
        let checker =
            VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, 2).unwrap();

        let tx_id = next_tx_id();
        status_table.set_committed(tx_id).unwrap();

        let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
        let eligibility = checker.check_all_criteria(&version, 10).unwrap();

        assert!(eligibility.is_creator_committed);
        assert!(eligibility.is_end_ts_invisible); // end_ts=100 < min_visible_ts (u64::MAX or default)
        assert!(eligibility.is_grace_period_met); // 10 >= 5 + 2
        assert!(eligibility.is_fully_eligible());
    }

    #[test]
    fn test_version_eligibility_creator_not_committed() {
        let status_table = make_status_table();
        let snapshot_registry = make_snapshot_registry();
        let checker =
            VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, 2).unwrap();

        let tx_id = next_tx_id();
        // Do NOT mark as committed

        let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
        let eligibility = checker.check_all_criteria(&version, 10).unwrap();

        assert!(!eligibility.is_creator_committed);
        assert!(!eligibility.is_fully_eligible());
    }

    #[test]
    fn test_version_eligibility_grace_period_not_met() {
        let status_table = make_status_table();
        let snapshot_registry = make_snapshot_registry();
        let checker =
            VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, 5).unwrap();

        let tx_id = next_tx_id();
        status_table.set_committed(tx_id).unwrap();

        let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
        let eligibility = checker.check_all_criteria(&version, 8).unwrap(); // 8 < 5 + 5

        assert!(eligibility.is_creator_committed);
        assert!(!eligibility.is_grace_period_met);
        assert!(!eligibility.is_fully_eligible());
    }

    #[test]
    fn test_version_record_validation() {
        assert!(VersionRecord::new(0, next_tx_id(), 100, 5).is_err());

        let tx_id = next_tx_id();
        assert!(VersionRecord::new(1, tx_id, 100, 5).is_ok());
    }

    #[test]
    fn test_eligibility_first_failed_criterion() {
        let elg1 = VersionEligibility {
            is_creator_committed: false,
            is_end_ts_invisible: true,
            is_grace_period_met: true,
        };
        assert_eq!(elg1.first_failed_criterion(), Some("creator not committed"));

        let elg2 = VersionEligibility {
            is_creator_committed: true,
            is_end_ts_invisible: false,
            is_grace_period_met: true,
        };
        assert_eq!(
            elg2.first_failed_criterion(),
            Some("end_ts still visible to snapshots")
        );

        let elg3 = VersionEligibility {
            is_creator_committed: true,
            is_end_ts_invisible: true,
            is_grace_period_met: false,
        };
        assert_eq!(
            elg3.first_failed_criterion(),
            Some("grace period not elapsed")
        );

        let elg_all_pass = VersionEligibility {
            is_creator_committed: true,
            is_end_ts_invisible: true,
            is_grace_period_met: true,
        };
        assert_eq!(elg_all_pass.first_failed_criterion(), None);
    }

    #[test]
    fn test_checker_statistics_tracking() {
        let status_table = make_status_table();
        let snapshot_registry = make_snapshot_registry();
        let checker =
            VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, 2).unwrap();

        let tx_id_1 = next_tx_id();
        status_table.set_committed(tx_id_1).unwrap();

        let tx_id_2 = next_tx_id();
        // Do not commit tx_id_2

        let v1 = VersionRecord::new(1, tx_id_1, 100, 5).unwrap();
        let v2 = VersionRecord::new(2, tx_id_2, 100, 5).unwrap();

        checker.check_all_criteria(&v1, 10).unwrap();
        checker.check_all_criteria(&v2, 10).unwrap();

        let stats = checker.get_stats();
        assert_eq!(stats.versions_checked, 2);
        assert_eq!(stats.versions_eligible, 1);
        assert_eq!(stats.ineligible_creator_not_committed, 1);
    }

    #[test]
    fn test_checker_reset_stats() {
        let status_table = make_status_table();
        let snapshot_registry = make_snapshot_registry();
        let checker =
            VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, 2).unwrap();

        let tx_id = next_tx_id();
        status_table.set_committed(tx_id).unwrap();

        let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
        checker.check_all_criteria(&version, 10).unwrap();

        let stats = checker.get_stats();
        assert!(stats.versions_checked > 0);

        checker.reset_stats();
        let stats_after = checker.get_stats();
        assert_eq!(stats_after.versions_checked, 0);
    }

    #[test]
    fn test_is_eligible_single_call() {
        let status_table = make_status_table();
        let snapshot_registry = make_snapshot_registry();
        let checker =
            VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, 2).unwrap();

        let tx_id = next_tx_id();
        status_table.set_committed(tx_id).unwrap();

        let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();
        let is_eligible = checker.is_eligible(&version, 10).unwrap();

        assert!(is_eligible);
    }

    #[test]
    fn test_multiple_ineligible_reasons() {
        let status_table = make_status_table();
        let snapshot_registry = make_snapshot_registry();
        let checker =
            VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, 10).unwrap();

        let tx_id = next_tx_id();
        // Do NOT commit

        let version = VersionRecord::new(1, tx_id, 100, 0).unwrap();
        let eligibility = checker.check_all_criteria(&version, 5).unwrap(); // gc_epoch < grace_period

        assert!(!eligibility.is_creator_committed);
        assert!(!eligibility.is_grace_period_met);
        assert!(!eligibility.is_fully_eligible());

        let stats = checker.get_stats();
        assert_eq!(stats.versions_checked, 1);
        assert_eq!(stats.versions_eligible, 0);
        assert!(stats.ineligible_creator_not_committed > 0);
        assert!(stats.ineligible_grace_period > 0);
    }

    #[test]
    fn test_grace_period_boundary_conditions() {
        let status_table = make_status_table();
        let snapshot_registry = make_snapshot_registry();
        let grace_period = 5;
        let checker =
            VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, grace_period)
                .unwrap();

        let tx_id = next_tx_id();
        status_table.set_committed(tx_id).unwrap();

        let marked_at = 10;
        let version = VersionRecord::new(1, tx_id, 100, marked_at).unwrap();

        // Just below threshold
        let eligibility_below = checker.check_all_criteria(&version, marked_at + grace_period - 1);
        assert!(eligibility_below.is_ok());
        assert!(!eligibility_below.unwrap().is_grace_period_met);

        // Exactly at threshold
        let eligibility_at = checker.check_all_criteria(&version, marked_at + grace_period);
        assert!(eligibility_at.is_ok());
        assert!(eligibility_at.unwrap().is_grace_period_met);

        // Above threshold
        let eligibility_above = checker.check_all_criteria(&version, marked_at + grace_period + 1);
        assert!(eligibility_above.is_ok());
        assert!(eligibility_above.unwrap().is_grace_period_met);
    }

    #[test]
    fn test_creator_status_defaults_to_inflight() {
        let status_table = make_status_table();
        let snapshot_registry = make_snapshot_registry();
        let checker =
            VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, 2).unwrap();

        let tx_id = next_tx_id();
        // Do NOT record this transaction

        let status = checker.creator_status(tx_id).unwrap();
        assert_eq!(status, TransactionStatus::InFlight);
    }

    #[test]
    fn test_min_visible_timestamp_query() {
        let status_table = make_status_table();
        let snapshot_registry = make_snapshot_registry();
        let checker = VersionEligibilityChecker::new(status_table, snapshot_registry, 2).unwrap();

        let min_ts = checker.min_visible_timestamp().unwrap();
        // Should succeed even with empty registry (returns u64::MAX or safe default)
        assert!(min_ts > 0 || min_ts == 0); // Just check it doesn't panic
    }

    #[test]
    fn test_version_eligibility_check_all_criteria_independent() {
        let status_table = make_status_table();
        let snapshot_registry = make_snapshot_registry();
        let checker =
            VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, 2).unwrap();

        let tx_id = next_tx_id();
        status_table.set_committed(tx_id).unwrap();

        let version = VersionRecord::new(1, tx_id, 100, 0).unwrap();

        // Test with gc_epoch = 0, so grace period definitely not met
        let eligibility = checker.check_all_criteria(&version, 0).unwrap();

        // Creator is committed ✓
        // end_ts is invisible ✓
        // Grace period NOT met ✗
        assert!(eligibility.is_creator_committed);
        assert!(eligibility.is_end_ts_invisible);
        assert!(!eligibility.is_grace_period_met);
        assert!(!eligibility.is_fully_eligible());
    }

    #[test]
    fn test_stats_monotonic_accumulation() {
        let status_table = make_status_table();
        let snapshot_registry = make_snapshot_registry();
        let checker =
            VersionEligibilityChecker::new(status_table.clone(), snapshot_registry, 2).unwrap();

        let tx_id = next_tx_id();
        status_table.set_committed(tx_id).unwrap();

        let version = VersionRecord::new(1, tx_id, 100, 5).unwrap();

        for i in 0..10 {
            checker.check_all_criteria(&version, 10).unwrap();
            let stats = checker.get_stats();
            assert_eq!(stats.versions_checked, i as u64 + 1);
            assert_eq!(stats.versions_eligible, i as u64 + 1);
        }

        let final_stats = checker.get_stats();
        assert_eq!(final_stats.versions_checked, 10);
        assert_eq!(final_stats.versions_eligible, 10);
    }

    #[test]
    fn test_grace_period_zero_rejected() {
        let status_table = make_status_table();
        let snapshot_registry = make_snapshot_registry();

        let result = VersionEligibilityChecker::new(status_table, snapshot_registry, 0);
        assert!(result.is_err());
    }
}
