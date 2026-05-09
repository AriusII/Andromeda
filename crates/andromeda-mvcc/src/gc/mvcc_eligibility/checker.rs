use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use andromeda_types::TransactionId;

use crate::active_snapshot_registry::ActiveSnapshotRegistry;
use crate::status::{TransactionStatus, TransactionStatusTable};

use super::types::{Timestamp, VersionEligibility, VersionEligibilityStats, VersionRecord};

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
    /// Aborted transactions are never visible to snapshots, so their versions
    /// can be reclaimed without the normal grace-period and visibility checks.
    pub fn is_aborted_version_eligible(&self, version: &VersionRecord) -> bool {
        let creator_status = self
            .status_table
            .status(version.creator_tx_id)
            .unwrap_or(TransactionStatus::InFlight);

        matches!(creator_status, TransactionStatus::RolledBack)
    }

    /// Check if a version created by an in-flight transaction is safe (never eligible).
    ///
    /// A version from an in-flight transaction must stay protected until the
    /// creator reaches a terminal status.
    pub fn is_in_flight_version_safe(&self, version: &VersionRecord) -> bool {
        let creator_status = self
            .status_table
            .status(version.creator_tx_id)
            .unwrap_or(TransactionStatus::InFlight);

        !matches!(creator_status, TransactionStatus::InFlight)
    }

    /// Determine eligibility with fast-path checks for aborted and in-flight versions.
    ///
    /// This method performs eligibility checks in the following order:
    /// 1. If version is from **aborted tx**: immediately eligible (return `true`)
    /// 2. If version is from **in-flight tx**: immediately ineligible (return `false`)
    /// 3. Otherwise: apply normal three-criterion check
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
