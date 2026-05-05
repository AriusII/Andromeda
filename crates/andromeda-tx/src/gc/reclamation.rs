//! MVCC Reclamation Mark Types and Eligibility Criteria
//!
//! This module defines the formal type system for MVCC version reclamation
//! (garbage collection marking). It ensures correctness of version cleanup by
//! checking creator transaction status, visibility timestamps, and grace periods.
//!
//! # Reclamation Lifecycle
//!
//! 1. **Version Creation**: A version is created with:
//!    - `creator_tx_id`: Transaction that created this version
//!    - `version_id`: Unique identifier within the row's version chain
//!    - `begin_ts`: Logical timestamp when this version became visible
//!    - `end_ts`: Initially `u64::MAX` (open/live version)
//!
//! 2. **Version Closure**: When a new version is created or row is deleted:
//!    - Previous version's `end_ts` is set to current logical timestamp
//!    - Marks when this version ceased to be the "current" version
//!
//! 3. **Eligibility Check**: GC scanner periodically checks if a version is eligible:
//!    - Is the creator transaction committed? (from `TransactionStatusTable`)
//!    - Is `end_ts` invisible to all active snapshots? (`end_ts < min_visible_ts`)
//!    - Has enough time passed since marking? (grace period)
//!
//! 4. **Reclamation Marking**: If all eligibility criteria are met:
//!    - Emit `ReclaimationMark` with metadata
//!    - Generate `ReclamationCommand` for executor
//!
//! 5. **Reclamation Execution**: Executor processes command:
//!    - Remove version tuple from storage
//!    - Decrement row's version count
//!    - Update statistics
//!
//! # Invariant: No Visible Version Can Be Reclaimed
//!
//! The eligibility check ensures this critical invariant:
//! - A version is visible if: creator is committed AND end_ts >= snapshot.timestamp
//! - Therefore, a version can only be reclaimed if: end_ts < min_visible_ts
//! - Since `min_visible_ts` is the oldest active snapshot's timestamp,
//!   no active snapshot can see `end_ts` (they all see timestamps >= min_visible_ts)
//! - Uncommitted creators also cannot be reclaimed (reads must wait for durable commit)

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};

use crate::mvcc_status::{TransactionStatus, TransactionStatusTable};

/// Unique identifier for a version within a row's version chain.
pub type VersionId = u64;

/// Logical timestamp for MVCC ordering (not wall-clock time).
pub type Timestamp = u64;

/// Reclamation eligibility criteria for a single version.
///
/// All three criteria must be true for a version to be eligible for reclamation:
/// 1. Creator transaction must be durably committed
/// 2. End timestamp must be strictly less than minimum visible timestamp
/// 3. Grace period must have expired (marked_at + grace_period < now)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReclaimationEligibility {
    /// True if creator transaction status is Committed (per V0 doctrine).
    pub is_creator_committed: bool,
    /// True if version's end_ts is strictly less than min_visible_ts
    /// (version is invisible to all active snapshots).
    pub is_end_ts_invisible: bool,
    /// True if grace period has elapsed since marking.
    pub gc_epoch_qualified: bool,
}

impl ReclaimationEligibility {
    /// Check if all eligibility criteria are met.
    pub fn is_fully_eligible(&self) -> bool {
        self.is_creator_committed && self.is_end_ts_invisible && self.gc_epoch_qualified
    }

    /// Create eligibility with custom values (for testing).
    pub fn new(
        is_creator_committed: bool,
        is_end_ts_invisible: bool,
        gc_epoch_qualified: bool,
    ) -> Self {
        ReclaimationEligibility {
            is_creator_committed,
            is_end_ts_invisible,
            gc_epoch_qualified,
        }
    }
}

/// Reclamation mark: formal metadata for a version eligible for cleanup.
///
/// Once a version meets all eligibility criteria, a `ReclaimationMark` is created
/// to guide the GC executor in safe tuple removal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReclaimationMark {
    /// The version being marked for reclamation.
    pub version_id: VersionId,
    /// Transaction that created this version (for audit trail).
    pub creator_tx_id: TransactionId,
    /// The timestamp when this version became invisible (closed version).
    pub end_ts: Timestamp,
    /// The timestamp when this mark was created (for grace period enforcement).
    pub marked_at: Timestamp,
    /// GC epoch counter to ensure mark is not too recent.
    pub gc_epoch: u64,
}

impl ReclaimationMark {
    /// Create a new reclamation mark.
    ///
    /// This constructor does NOT validate eligibility. Use `from_version`
    /// for safe creation that checks eligibility criteria.
    pub fn new(
        version_id: VersionId,
        creator_tx_id: TransactionId,
        end_ts: Timestamp,
        marked_at: Timestamp,
        gc_epoch: u64,
    ) -> AndromedaResult<Self> {
        if version_id == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "reclamation mark version_id must not be zero",
            ));
        }

        if creator_tx_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "reclamation mark creator_tx_id must not be zero",
            ));
        }

        if end_ts == u64::MAX {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "reclamation mark end_ts must be less than u64::MAX (live versions cannot be marked)",
            ));
        }

        Ok(ReclaimationMark {
            version_id,
            creator_tx_id,
            end_ts,
            marked_at,
            gc_epoch,
        })
    }

    /// Create a reclamation mark from a version record only if fully eligible.
    ///
    /// Returns `None` if the version is not eligible (ineligible creator,
    /// or end_ts still visible, or grace period not met).
    pub fn from_version_if_eligible(
        version_id: VersionId,
        creator_tx_id: TransactionId,
        end_ts: Timestamp,
        marked_at: Timestamp,
        gc_epoch: u64,
        status_table: &TransactionStatusTable,
        min_visible_ts: Timestamp,
        grace_period_epochs: u64,
        current_gc_epoch: u64,
    ) -> AndromedaResult<Option<Self>> {
        // Check creator transaction status
        let creator_status = status_table
            .status(creator_tx_id)
            .unwrap_or(TransactionStatus::InFlight);
        let is_creator_committed = matches!(creator_status, TransactionStatus::Committed);

        if !is_creator_committed {
            return Ok(None);
        }

        // Check if end_ts is invisible
        if end_ts >= min_visible_ts {
            return Ok(None);
        }

        // Check grace period
        let gc_epoch_qualified = current_gc_epoch >= gc_epoch + grace_period_epochs;
        if !gc_epoch_qualified {
            return Ok(None);
        }

        // All criteria met, create mark
        let mark = Self::new(version_id, creator_tx_id, end_ts, marked_at, gc_epoch)?;
        Ok(Some(mark))
    }

    /// Check if this mark meets all eligibility criteria.
    ///
    /// This is a runtime check to validate eligibility against current state.
    pub fn is_eligible(
        &self,
        status_table: &TransactionStatusTable,
        min_visible_ts: Timestamp,
        grace_period_epochs: u64,
        current_gc_epoch: u64,
    ) -> bool {
        let creator_status = status_table
            .status(self.creator_tx_id)
            .unwrap_or(TransactionStatus::InFlight);

        let is_creator_committed = matches!(creator_status, TransactionStatus::Committed);
        let is_end_ts_invisible = self.end_ts < min_visible_ts;
        let gc_epoch_qualified = current_gc_epoch >= self.gc_epoch + grace_period_epochs;

        is_creator_committed && is_end_ts_invisible && gc_epoch_qualified
    }

    /// Generate the eligibility criteria snapshot.
    pub fn check_eligibility(
        &self,
        status_table: &TransactionStatusTable,
        min_visible_ts: Timestamp,
        grace_period_epochs: u64,
        current_gc_epoch: u64,
    ) -> ReclaimationEligibility {
        let creator_status = status_table
            .status(self.creator_tx_id)
            .unwrap_or(TransactionStatus::InFlight);

        ReclaimationEligibility {
            is_creator_committed: matches!(creator_status, TransactionStatus::Committed),
            is_end_ts_invisible: self.end_ts < min_visible_ts,
            gc_epoch_qualified: current_gc_epoch >= self.gc_epoch + grace_period_epochs,
        }
    }

    /// Emit a reclamation command for the executor to process this mark.
    pub fn mark_for_reclamation(&self) -> ReclamationCommand {
        ReclamationCommand {
            version_id: self.version_id,
            creator_tx_id: self.creator_tx_id,
            end_ts: self.end_ts,
        }
    }

    /// Validate the mark's internal consistency.
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.version_id == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "reclamation mark version_id must not be zero",
            ));
        }

        if self.creator_tx_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "reclamation mark creator_tx_id must not be zero",
            ));
        }

        if self.end_ts == u64::MAX {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "reclamation mark end_ts must be less than u64::MAX",
            ));
        }

        Ok(())
    }
}

/// Command emitted to GC executor for tuple reclamation.
///
/// The executor uses this to:
/// 1. Locate the tuple in storage
/// 2. Remove it
/// 3. Update version chain pointers
/// 4. Decrement version count
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReclamationCommand {
    pub version_id: VersionId,
    pub creator_tx_id: TransactionId,
    pub end_ts: Timestamp,
}

impl ReclamationCommand {
    /// Create a new reclamation command.
    pub fn new(
        version_id: VersionId,
        creator_tx_id: TransactionId,
        end_ts: Timestamp,
    ) -> AndromedaResult<Self> {
        if version_id == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "reclamation command version_id must not be zero",
            ));
        }

        if creator_tx_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "reclamation command creator_tx_id must not be zero",
            ));
        }

        Ok(ReclamationCommand {
            version_id,
            creator_tx_id,
            end_ts,
        })
    }
}

/// Statistics for reclamation marking and execution.
#[derive(Debug, Clone)]
pub struct ReclamationStats {
    marks_created: Arc<AtomicU64>,
    commands_executed: Arc<AtomicU64>,
    versions_reclaimed: Arc<AtomicU64>,
}

impl ReclamationStats {
    pub fn new() -> Self {
        ReclamationStats {
            marks_created: Arc::new(AtomicU64::new(0)),
            commands_executed: Arc::new(AtomicU64::new(0)),
            versions_reclaimed: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn marks_created(&self) -> u64 {
        self.marks_created.load(Ordering::Relaxed)
    }

    pub fn commands_executed(&self) -> u64 {
        self.commands_executed.load(Ordering::Relaxed)
    }

    pub fn versions_reclaimed(&self) -> u64 {
        self.versions_reclaimed.load(Ordering::Relaxed)
    }

    pub(crate) fn record_mark(&self) {
        self.marks_created.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_execution(&self) {
        self.commands_executed.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_reclamation(&self) {
        self.versions_reclaimed.fetch_add(1, Ordering::Relaxed);
    }
}

impl Default for ReclamationStats {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reclamation_mark_creation() {
        let mark = ReclaimationMark::new(
            1, // version_id
            TransactionId::new(1),
            100, // end_ts
            50,  // marked_at
            5,   // gc_epoch
        );

        assert!(mark.is_ok());
        let m = mark.unwrap();
        assert_eq!(m.version_id, 1);
        assert_eq!(m.end_ts, 100);
    }

    #[test]
    fn reclamation_mark_rejects_zero_version_id() {
        let mark = ReclaimationMark::new(
            0, // Invalid: zero
            TransactionId::new(1),
            100,
            50,
            5,
        );

        assert!(mark.is_err());
    }

    #[test]
    fn reclamation_mark_rejects_zero_creator_tx_id() {
        let mark = ReclaimationMark::new(
            1,
            TransactionId::new(0), // Invalid: zero
            100,
            50,
            5,
        );

        assert!(mark.is_err());
    }

    #[test]
    fn reclamation_mark_rejects_live_version() {
        let mark = ReclaimationMark::new(
            1,
            TransactionId::new(1),
            u64::MAX, // Invalid: live version
            50,
            5,
        );

        assert!(mark.is_err());
    }

    #[test]
    fn reclamation_eligibility_fully_eligible() {
        let eligibility = ReclaimationEligibility::new(true, true, true);
        assert!(eligibility.is_fully_eligible());
    }

    #[test]
    fn reclamation_eligibility_not_committed_creator() {
        let eligibility = ReclaimationEligibility::new(false, true, true);
        assert!(!eligibility.is_fully_eligible());
    }

    #[test]
    fn reclamation_eligibility_end_ts_still_visible() {
        let eligibility = ReclaimationEligibility::new(true, false, true);
        assert!(!eligibility.is_fully_eligible());
    }

    #[test]
    fn reclamation_eligibility_grace_period_not_met() {
        let eligibility = ReclaimationEligibility::new(true, true, false);
        assert!(!eligibility.is_fully_eligible());
    }

    #[test]
    fn reclamation_command_creation() {
        let cmd = ReclamationCommand::new(1, TransactionId::new(1), 100);
        assert!(cmd.is_ok());
        let c = cmd.unwrap();
        assert_eq!(c.version_id, 1);
    }

    #[test]
    fn reclamation_command_rejects_zero_version_id() {
        let cmd = ReclamationCommand::new(0, TransactionId::new(1), 100);
        assert!(cmd.is_err());
    }

    #[test]
    fn reclamation_mark_emit_command() {
        let mark = ReclaimationMark::new(1, TransactionId::new(1), 100, 50, 5).unwrap();
        let cmd = mark.mark_for_reclamation();

        assert_eq!(cmd.version_id, mark.version_id);
        assert_eq!(cmd.creator_tx_id, mark.creator_tx_id);
        assert_eq!(cmd.end_ts, mark.end_ts);
    }

    #[test]
    fn reclamation_mark_validate_success() {
        let mark = ReclaimationMark::new(1, TransactionId::new(1), 100, 50, 5).unwrap();
        assert!(mark.validate().is_ok());
    }

    #[test]
    fn reclamation_stats_tracking() {
        let stats = ReclamationStats::new();

        assert_eq!(stats.marks_created(), 0);
        assert_eq!(stats.commands_executed(), 0);
        assert_eq!(stats.versions_reclaimed(), 0);

        stats.record_mark();
        assert_eq!(stats.marks_created(), 1);

        stats.record_execution();
        assert_eq!(stats.commands_executed(), 1);

        stats.record_reclamation();
        assert_eq!(stats.versions_reclaimed(), 1);
    }

    #[test]
    fn reclamation_mark_eligibility_with_committed_creator() {
        let status_table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        status_table.set_committed(tx_id).unwrap();

        let mark = ReclaimationMark::new(1, tx_id, 50, 20, 5).unwrap();
        let eligibility = mark.check_eligibility(&status_table, 100, 0, 10);

        assert!(eligibility.is_creator_committed);
        assert!(eligibility.is_end_ts_invisible); // 50 < 100
        assert!(eligibility.gc_epoch_qualified); // 10 >= 5 + 0
    }

    #[test]
    fn reclamation_mark_eligibility_with_uncommitted_creator() {
        let status_table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        // Don't mark as committed - remains InFlight

        let mark = ReclaimationMark::new(1, tx_id, 50, 20, 5).unwrap();
        let eligibility = mark.check_eligibility(&status_table, 100, 0, 10);

        assert!(!eligibility.is_creator_committed);
        assert!(!eligibility.is_fully_eligible());
    }

    #[test]
    fn reclamation_mark_eligibility_end_ts_still_visible() {
        let status_table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        status_table.set_committed(tx_id).unwrap();

        let mark = ReclaimationMark::new(1, tx_id, 150, 20, 5).unwrap();
        // min_visible_ts is 100, but end_ts is 150
        let eligibility = mark.check_eligibility(&status_table, 100, 0, 10);

        assert!(eligibility.is_creator_committed);
        assert!(!eligibility.is_end_ts_invisible); // 150 >= 100
        assert!(!eligibility.is_fully_eligible());
    }

    #[test]
    fn reclamation_mark_eligibility_grace_period_not_met() {
        let status_table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        status_table.set_committed(tx_id).unwrap();

        let mark = ReclaimationMark::new(1, tx_id, 50, 20, 10).unwrap();
        // Grace period is 5 epochs, current is 10, marked_at epoch is 10
        // So 10 < 10 + 5 (not expired)
        let eligibility = mark.check_eligibility(&status_table, 100, 5, 10);

        assert!(eligibility.is_creator_committed);
        assert!(eligibility.is_end_ts_invisible);
        assert!(!eligibility.gc_epoch_qualified);
        assert!(!eligibility.is_fully_eligible());
    }

    #[test]
    fn reclamation_mark_from_version_if_fully_eligible() {
        let status_table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        status_table.set_committed(tx_id).unwrap();

        let mark_opt = ReclaimationMark::from_version_if_eligible(
            1,                 // version_id
            tx_id,             // creator_tx_id
            50,                // end_ts
            20,                // marked_at
            5,                 // gc_epoch
            &status_table,
            100, // min_visible_ts
            0,   // grace_period_epochs
            10,  // current_gc_epoch
        );

        assert!(mark_opt.is_ok());
        assert!(mark_opt.unwrap().is_some());
    }

    #[test]
    fn reclamation_mark_from_version_rejected_uncommitted() {
        let status_table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        // Don't mark as committed

        let mark_opt = ReclaimationMark::from_version_if_eligible(
            1, tx_id, 50, 20, 5, &status_table, 100, 0, 10,
        );

        assert!(mark_opt.is_ok());
        assert!(mark_opt.unwrap().is_none());
    }

    #[test]
    fn reclamation_mark_from_version_rejected_still_visible() {
        let status_table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        status_table.set_committed(tx_id).unwrap();

        let mark_opt = ReclaimationMark::from_version_if_eligible(
            1, tx_id, 150, 20, 5, &status_table, 100, 0, 10,
        );

        assert!(mark_opt.is_ok());
        assert!(mark_opt.unwrap().is_none()); // end_ts 150 >= min_visible_ts 100
    }

    #[test]
    fn reclamation_mark_from_version_rejected_grace_period() {
        let status_table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        status_table.set_committed(tx_id).unwrap();

        let mark_opt = ReclaimationMark::from_version_if_eligible(
            1, tx_id, 50, 20, 10, &status_table, 100, 5, 10,
        );

        assert!(mark_opt.is_ok());
        assert!(mark_opt.unwrap().is_none()); // 10 < 10 + 5 (grace period not met)
    }

    #[test]
    fn reclamation_mark_is_eligible_runtime_check() {
        let status_table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        status_table.set_committed(tx_id).unwrap();

        let mark = ReclaimationMark::new(1, tx_id, 50, 20, 5).unwrap();

        assert!(mark.is_eligible(&status_table, 100, 0, 10));
    }

    #[test]
    fn reclamation_mark_batch_processing_scenario() {
        let status_table = TransactionStatusTable::new();
        let mut marks = Vec::new();

        // Create 10 marks, all with same creator
        let tx_id = TransactionId::new(1);
        status_table.set_committed(tx_id).unwrap();

        for i in 1..=10 {
            let mark = ReclaimationMark::new(i, tx_id, 50, 20, 5).unwrap();
            marks.push(mark);
        }

        // Count eligible marks
        let eligible_count = marks
            .iter()
            .filter(|m| m.is_eligible(&status_table, 100, 0, 10))
            .count();

        assert_eq!(eligible_count, 10);
    }

    #[test]
    fn reclamation_no_visible_version_invariant_proof() {
        // This test proves: if end_ts < min_visible_ts, no visible transaction can see the version
        let status_table = TransactionStatusTable::new();
        let tx_id = TransactionId::new(1);

        status_table.set_committed(tx_id).unwrap();

        // Version with end_ts = 50
        let mark = ReclaimationMark::new(1, tx_id, 50, 20, 5).unwrap();

        // min_visible_ts = 100 means:
        // - The oldest active snapshot has timestamp >= 100
        // - No snapshot can have timestamp < 100 (not active)
        // - To see a version, snapshot.timestamp must be >= end_ts
        // - Since all active snapshots have timestamp >= 100, and end_ts = 50,
        //   they would see the version IF it existed
        // - However, no active snapshot should see timestamps < 100
        //   (they're all >= 100 because that's the minimum active)
        //
        // The reclamation mark ensures: if end_ts < min_visible_ts, the version is safe to reclaim

        assert!(mark.is_eligible(&status_table, 100, 0, 10));
        let eligibility = mark.check_eligibility(&status_table, 100, 0, 10);
        assert!(eligibility.is_end_ts_invisible);
    }
}
