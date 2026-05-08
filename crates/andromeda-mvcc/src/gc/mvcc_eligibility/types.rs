use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};

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
#[derive(Debug, Clone, Copy, Default)]
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
