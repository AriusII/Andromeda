use crate::Lsn;
use std::fmt;

/// Decision result: can a segment be safely garbage-collected?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReclaimabilityDecision {
    /// Segment can be garbage-collected.
    Reclaimable,

    /// Segment is required for crash recovery.
    BlockedByRecovery {
        segment_start_lsn: Lsn,
        required_recovery_lsn: Lsn,
    },

    /// Segment overlaps the active snapshot visibility boundary.
    BlockedByVisibility {
        segment_end_lsn: Lsn,
        min_active_snapshot_lsn: Lsn,
    },

    /// Segment has not yet been durably received by all configured standbys.
    BlockedByReplication {
        segment_end_lsn: Lsn,
        min_standby_received_lsn: Lsn,
    },

    /// Segment falls within the point-in-time recovery retention window.
    BlockedByPitrRetention {
        segment_end_lsn: Lsn,
        pitr_retention_lsn: Lsn,
    },
}

impl ReclaimabilityDecision {
    /// Check if the segment is reclaimable.
    pub fn is_reclaimable(self) -> bool {
        matches!(self, Self::Reclaimable)
    }

    /// Human-readable reason for the decision.
    pub fn reason(self) -> &'static str {
        match self {
            Self::Reclaimable => {
                "segment is outside all retention boundaries and can be safely garbage-collected"
            },
            Self::BlockedByRecovery { .. } => {
                "segment is required for crash recovery (at or before recovery boundary)"
            },
            Self::BlockedByVisibility { .. } => {
                "segment contains data visible to at least one active snapshot"
            },
            Self::BlockedByReplication { .. } => {
                "segment has not been durably received by all configured standbys"
            },
            Self::BlockedByPitrRetention { .. } => {
                "segment falls within the point-in-time recovery retention window"
            },
        }
    }

    /// Extract the blocking LSN value for diagnostics.
    pub fn blocking_lsn(self) -> Option<Lsn> {
        match self {
            Self::Reclaimable => None,
            Self::BlockedByRecovery {
                required_recovery_lsn,
                ..
            } => Some(required_recovery_lsn),
            Self::BlockedByVisibility {
                min_active_snapshot_lsn,
                ..
            } => Some(min_active_snapshot_lsn),
            Self::BlockedByReplication {
                min_standby_received_lsn,
                ..
            } => Some(min_standby_received_lsn),
            Self::BlockedByPitrRetention {
                pitr_retention_lsn, ..
            } => Some(pitr_retention_lsn),
        }
    }
}

impl fmt::Display for ReclaimabilityDecision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.reason())
    }
}
