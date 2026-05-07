use crate::Lsn;
use andromeda_core::AndromedaResult;

use super::{ReclaimabilityDecision, ReclaimabilityEvidence, RetentionBoundaryPolicy};
use crate::write_ahead_log::{gc::WalGcCandidate, shipping::WalReplicaSafeLsnTracker};

/// WAL segment reclaimability policy enforcement.
///
/// Implementations must evaluate all retention boundaries from a consistent
/// policy snapshot and return the exact boundary that blocks reclaim.
pub trait WalSegmentReclaimability: Send + Sync {
    /// Determine if a segment can be safely garbage-collected.
    fn can_reclaim_segment(&self, segment: &WalGcCandidate) -> ReclaimabilityDecision;

    /// Get a snapshot of all current retention boundaries.
    ///
    /// This is intended for diagnostics and test assertions. It may not
    /// represent the same instant as a concurrent `can_reclaim_segment()` call.
    fn retention_boundaries(&self) -> RetentionBoundaryPolicy;
}

/// Default implementation combining all retention boundary checks.
pub struct DefaultReclaimabilityPolicy {
    boundaries: RetentionBoundaryPolicy,
}

impl DefaultReclaimabilityPolicy {
    /// Create a policy with given retention boundaries.
    pub fn new(boundaries: RetentionBoundaryPolicy) -> AndromedaResult<Self> {
        boundaries.validate()?;
        Ok(Self { boundaries })
    }

    /// Create from individual LSN components.
    pub fn with_lsns(
        required_recovery_lsn: Lsn,
        min_active_snapshot_lsn: Lsn,
        min_standby_received_lsn: Lsn,
        pitr_retention_lsn: Lsn,
    ) -> AndromedaResult<Self> {
        let boundaries = RetentionBoundaryPolicy::new(
            required_recovery_lsn,
            min_active_snapshot_lsn,
            min_standby_received_lsn,
            pitr_retention_lsn,
        )?;
        Ok(Self { boundaries })
    }

    /// Create a policy whose replication boundary is derived from required
    /// replica shipping ACKs.
    pub fn with_replica_safe_lsn_tracker(
        required_recovery_lsn: Lsn,
        min_active_snapshot_lsn: Lsn,
        replica_safe_lsn_tracker: &WalReplicaSafeLsnTracker,
        pitr_retention_lsn: Lsn,
    ) -> AndromedaResult<Self> {
        Self::with_lsns(
            required_recovery_lsn,
            min_active_snapshot_lsn,
            replica_safe_lsn_tracker.retention_boundary_lsn(),
            pitr_retention_lsn,
        )
    }
}

impl WalSegmentReclaimability for DefaultReclaimabilityPolicy {
    fn can_reclaim_segment(&self, segment: &WalGcCandidate) -> ReclaimabilityDecision {
        ReclaimabilityEvidence::new(segment, self.boundaries).evaluate()
    }

    fn retention_boundaries(&self) -> RetentionBoundaryPolicy {
        self.boundaries
    }
}
