/// Reclamation eligibility criteria for a single version.
///
/// All three criteria must be true for a version to be eligible for reclamation:
/// 1. Creator transaction must be durably committed
/// 2. End timestamp must be strictly less than minimum visible timestamp
/// 3. Grace period must have expired (marked_at + grace_period < now)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReclamationEligibility {
    /// True if creator transaction status is Committed (per V0 doctrine).
    pub is_creator_committed: bool,
    /// True if version's end_ts is strictly less than min_visible_ts
    /// (version is invisible to all active snapshots).
    pub is_end_ts_invisible: bool,
    /// True if grace period has elapsed since marking.
    pub gc_epoch_qualified: bool,
}

impl ReclamationEligibility {
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
        ReclamationEligibility {
            is_creator_committed,
            is_end_ts_invisible,
            gc_epoch_qualified,
        }
    }
}
