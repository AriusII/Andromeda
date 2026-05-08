use crate::Lsn;

/// Result of eligibility checking for a WAL segment.
///
/// Indicates whether a segment can be garbage collected and why (if ineligible).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EligibilityResult {
    /// Segment is safe for garbage collection
    Eligible,
    /// Segment is not wholly older than the oldest active snapshot visibility boundary
    BlockedByVisibility {
        segment_end_lsn: Lsn,
        min_active_snapshot_lsn: Lsn,
    },
    /// Segment is required by crash recovery
    BlockedByRecovery {
        segment_start_lsn: Lsn,
        required_recovery_lsn: Lsn,
    },
}

impl EligibilityResult {
    /// Check if the result indicates eligibility
    pub fn is_eligible(self) -> bool {
        matches!(self, Self::Eligible)
    }

    /// Produce a human-readable description of the reason
    pub fn reason(self) -> &'static str {
        match self {
            Self::Eligible => {
                "segment is older than the oldest active snapshot and is not needed for recovery"
            }
            Self::BlockedByVisibility { .. } => {
                "segment is not older than the oldest active snapshot visibility boundary"
            }
            Self::BlockedByRecovery { .. } => "segment is required for crash recovery",
        }
    }
}
