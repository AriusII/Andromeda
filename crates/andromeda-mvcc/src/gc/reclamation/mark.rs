use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};

use crate::status::{TransactionStatus, TransactionStatusTable};

use super::{ReclamationCommand, ReclamationEligibility, Timestamp, VersionId};

/// Reclamation mark: formal metadata for a version eligible for cleanup.
///
/// Once a version meets all eligibility criteria, a `ReclamationMark` is created
/// to guide the GC executor in safe tuple removal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReclamationMark {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReclamationMarkCandidate {
    pub version_id: VersionId,
    pub creator_tx_id: TransactionId,
    pub end_ts: Timestamp,
    pub marked_at: Timestamp,
    pub gc_epoch: u64,
}

impl ReclamationMarkCandidate {
    pub const fn new(
        version_id: VersionId,
        creator_tx_id: TransactionId,
        end_ts: Timestamp,
        marked_at: Timestamp,
        gc_epoch: u64,
    ) -> Self {
        Self {
            version_id,
            creator_tx_id,
            end_ts,
            marked_at,
            gc_epoch,
        }
    }
}

impl ReclamationMark {
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

        Ok(ReclamationMark {
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
        candidate: ReclamationMarkCandidate,
        status_table: &TransactionStatusTable,
        min_visible_ts: Timestamp,
        grace_period_epochs: u64,
        current_gc_epoch: u64,
    ) -> AndromedaResult<Option<Self>> {
        let creator_status = status_table
            .status(candidate.creator_tx_id)
            .unwrap_or(TransactionStatus::InFlight);
        let is_creator_committed = matches!(creator_status, TransactionStatus::Committed);

        if !is_creator_committed {
            return Ok(None);
        }

        if candidate.end_ts >= min_visible_ts {
            return Ok(None);
        }

        let gc_epoch_qualified = current_gc_epoch >= candidate.gc_epoch + grace_period_epochs;
        if !gc_epoch_qualified {
            return Ok(None);
        }

        let mark = Self::new(
            candidate.version_id,
            candidate.creator_tx_id,
            candidate.end_ts,
            candidate.marked_at,
            candidate.gc_epoch,
        )?;
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
    ) -> ReclamationEligibility {
        let creator_status = status_table
            .status(self.creator_tx_id)
            .unwrap_or(TransactionStatus::InFlight);

        ReclamationEligibility {
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
