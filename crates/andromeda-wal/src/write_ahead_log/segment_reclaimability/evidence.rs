use crate::Lsn;

use super::{ReclaimabilityDecision, RetentionBoundaryPolicy};
use crate::write_ahead_log::gc::WalGcCandidate;

/// Immutable evidence used for a single segment reclaimability decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReclaimabilityEvidence {
    /// Candidate segment creation LSN.
    pub segment_start_lsn: Lsn,
    /// Candidate segment sealing LSN.
    pub segment_end_lsn: Lsn,
    /// Retention boundaries observed for this decision.
    pub boundaries: RetentionBoundaryPolicy,
}

impl ReclaimabilityEvidence {
    /// Capture evidence for a candidate segment and retention policy snapshot.
    pub fn new(segment: &WalGcCandidate, boundaries: RetentionBoundaryPolicy) -> Self {
        Self {
            segment_start_lsn: segment.creation_lsn,
            segment_end_lsn: segment.sealing_lsn,
            boundaries,
        }
    }

    /// Evaluate reclaimability using deterministic boundary precedence.
    pub fn evaluate(self) -> ReclaimabilityDecision {
        if self.segment_start_lsn <= self.boundaries.required_recovery_lsn {
            return ReclaimabilityDecision::BlockedByRecovery {
                segment_start_lsn: self.segment_start_lsn,
                required_recovery_lsn: self.boundaries.required_recovery_lsn,
            };
        }

        if self.segment_end_lsn >= self.boundaries.min_active_snapshot_lsn {
            return ReclaimabilityDecision::BlockedByVisibility {
                segment_end_lsn: self.segment_end_lsn,
                min_active_snapshot_lsn: self.boundaries.min_active_snapshot_lsn,
            };
        }

        if self.segment_end_lsn > self.boundaries.min_standby_received_lsn {
            return ReclaimabilityDecision::BlockedByReplication {
                segment_end_lsn: self.segment_end_lsn,
                min_standby_received_lsn: self.boundaries.min_standby_received_lsn,
            };
        }

        if self.segment_end_lsn >= self.boundaries.pitr_retention_lsn {
            return ReclaimabilityDecision::BlockedByPitrRetention {
                segment_end_lsn: self.segment_end_lsn,
                pitr_retention_lsn: self.boundaries.pitr_retention_lsn,
            };
        }

        ReclaimabilityDecision::Reclaimable
    }
}
