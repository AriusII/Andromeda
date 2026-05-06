//! WAL Segment Reclaimability Policy: Unified Retention Boundary Tracking
//!
//! # Overview
//!
//! This module implements the complete WAL segment garbage collection policy
//! that respects all four durability invariants:
//!
//! 1. **Crash Recovery**: Segments before `required_recovery_lsn` may be needed
//! 2. **Visibility**: Segments at or after `min_active_snapshot_lsn` are visible
//! 3. **Replication**: Segments not yet received by all standbys must be retained
//! 4. **PITR Archive**: Segments within retention window must be preserved
//!
//! # Architecture
//!
//! ```text
//! RetentionBoundaryPolicy
//!   ├── required_recovery_lsn (from manifest)
//!   ├── min_active_snapshot_lsn (from snapshot registry)
//!   ├── min_standby_received_lsn (from HA quorum)
//!   └── pitr_retention_lsn (from backup config)
//!
//! WalSegmentReclaimability trait
//!   ├── can_reclaim_segment(candidate) -> ReclaimabilityDecision
//!   └── retention_boundaries() -> RetentionBoundaryPolicy
//! ```
//!
//! # Invariants Maintained
//!
//! - A segment is never reclaimed if it may be needed for crash recovery
//! - A segment is never reclaimed if it contains an active snapshot boundary
//! - A segment is never reclaimed before all configured replicas receive it
//! - A segment is never reclaimed if within the PITR retention window
//! - All checks are atomic with respect to the reclaim decision
//! - No unsafe code

use crate::Lsn;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use std::fmt;

use super::WalGcCandidate;

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

/// Decision result: can segment be safely garbage-collected?
///
/// Indicates whether a segment is reclaimable and which retention boundary
/// blocks it (if any). Designed for deterministic audit logging and traceability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReclaimabilityDecision {
    /// Segment can be garbage-collected (all retention boundaries passed)
    Reclaimable,

    /// Blocked: segment is required for crash recovery
    ///
    /// This means the segment starts at or before the recovery boundary LSN.
    BlockedByRecovery {
        segment_start_lsn: Lsn,
        required_recovery_lsn: Lsn,
    },

    /// Blocked: segment contains or overlaps with active snapshot visibility boundary
    ///
    /// This means at least one active snapshot can still see records in this segment.
    BlockedByVisibility {
        segment_end_lsn: Lsn,
        min_active_snapshot_lsn: Lsn,
    },

    /// Blocked: segment has not yet been received by all configured standbys
    ///
    /// In HA/DR configurations, all replicas must durably receive a segment
    /// before the primary can reclaim it. This prevents replication gaps.
    BlockedByReplication {
        segment_end_lsn: Lsn,
        min_standby_received_lsn: Lsn,
    },

    /// Blocked: segment falls within the point-in-time recovery (PITR) retention window
    ///
    /// PITR requirements may mandate keeping segments for a configurable time or LSN
    /// range to support recovery to any point within the window.
    BlockedByPitrRetention {
        segment_end_lsn: Lsn,
        pitr_retention_lsn: Lsn,
    },
}

impl ReclaimabilityDecision {
    /// Check if the segment is reclaimable (passes all retention checks)
    pub fn is_reclaimable(self) -> bool {
        matches!(self, Self::Reclaimable)
    }

    /// Human-readable reason for the decision
    pub fn reason(self) -> &'static str {
        match self {
            Self::Reclaimable => {
                "segment is outside all retention boundaries and can be safely garbage-collected"
            }
            Self::BlockedByRecovery { .. } => {
                "segment is required for crash recovery (at or before recovery boundary)"
            }
            Self::BlockedByVisibility { .. } => {
                "segment contains data visible to at least one active snapshot"
            }
            Self::BlockedByReplication { .. } => {
                "segment has not been durably received by all configured standbys"
            }
            Self::BlockedByPitrRetention { .. } => {
                "segment falls within the point-in-time recovery retention window"
            }
        }
    }

    /// Extract the blocking LSN value for diagnostics
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

/// All four LSN retention boundaries for WAL GC policy.
///
/// These boundaries are consulted on every GC candidate check to ensure
/// that no segment is reclaimed prematurely. Each boundary is independent
/// and represents a different durability guarantee.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionBoundaryPolicy {
    /// Crash recovery floor (from manifest snapshot.required_wal_start_lsn):
    /// Segments with start_lsn <= this value may be needed for recovery.
    pub required_recovery_lsn: Lsn,

    /// Oldest active snapshot LSN (from snapshot registry):
    /// Segments with end_lsn >= this value contain visible data.
    pub min_active_snapshot_lsn: Lsn,

    /// Minimum LSN received by all configured standbys (from HA quorum):
    /// Segments with end_lsn >= this value have not been fully replicated.
    pub min_standby_received_lsn: Lsn,

    /// PITR retention window boundary (from backup policy):
    /// Segments with end_lsn >= this value must be kept for point-in-time recovery.
    pub pitr_retention_lsn: Lsn,
}

impl RetentionBoundaryPolicy {
    /// Create a new retention policy with all four boundaries.
    ///
    /// # Validation
    ///
    /// - `required_recovery_lsn` must not exceed `min_active_snapshot_lsn`
    ///   (recovery boundary must be older than snapshots)
    /// - `min_standby_received_lsn` is independent (HA may lag behind or ahead)
    /// - `pitr_retention_lsn` is independent of active snapshots; PITR policy can retain an
    ///   older range while active snapshots have already advanced beyond it.
    ///
    /// # Errors
    ///
    /// Returns error if LSN ordering invariants are violated.
    pub fn new(
        required_recovery_lsn: Lsn,
        min_active_snapshot_lsn: Lsn,
        min_standby_received_lsn: Lsn,
        pitr_retention_lsn: Lsn,
    ) -> AndromedaResult<Self> {
        // Validate ordering: recovery <= snapshot. Standby and PITR boundaries are independent.
        if required_recovery_lsn > min_active_snapshot_lsn {
            return Err(storage_error(
                "required_recovery_lsn must not exceed min_active_snapshot_lsn",
            ));
        }

        Ok(Self {
            required_recovery_lsn,
            min_active_snapshot_lsn,
            min_standby_received_lsn,
            pitr_retention_lsn,
        })
    }

    /// The effective upper GC boundary: minimum of all four boundaries.
    ///
    /// A segment can only be reclaimed if all its records are strictly
    /// before this LSN. Segments at or after this LSN must be retained.
    pub fn gc_boundary_lsn(&self) -> Lsn {
        std::cmp::min(
            std::cmp::min(self.min_active_snapshot_lsn, self.min_standby_received_lsn),
            self.pitr_retention_lsn,
        )
    }

    /// Validate internal consistency of the policy.
    ///
    /// Checks that all LSN ordering invariants are maintained.
    /// Useful for periodic validation during recovery or audit.
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.required_recovery_lsn > self.min_active_snapshot_lsn {
            return Err(storage_error(
                "recovery LSN exceeds snapshot LSN (invariant violation)",
            ));
        }
        Ok(())
    }
}

/// WAL segment reclaimability policy enforcement.
///
/// This trait abstracts the decision logic for determining whether a segment
/// can be safely garbage-collected. Implementations must check all four
/// retention boundaries atomically.
pub trait WalSegmentReclaimability: Send + Sync {
    /// Determine if a segment can be safely garbage-collected.
    ///
    /// This method must check all four retention boundaries and return a
    /// decision that indicates either reclaimability or the specific boundary
    /// that blocks the reclaim.
    ///
    /// # Atomicity
    ///
    /// The decision must be atomic: all four boundaries are consulted
    /// consistently without the possibility of them changing between checks.
    ///
    /// # Returns
    ///
    /// - `ReclaimabilityDecision::Reclaimable` if all boundaries pass
    /// - Otherwise, a specific blocking reason
    fn can_reclaim_segment(&self, segment: &WalGcCandidate) -> ReclaimabilityDecision;

    /// Get a snapshot of all current retention boundaries.
    ///
    /// Useful for diagnostics, logging, and test assertions.
    /// May not reflect the same instant as a concurrent `can_reclaim_segment()` call.
    fn retention_boundaries(&self) -> RetentionBoundaryPolicy;
}

/// Default implementation combining all four GC boundary checks.
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
}

impl WalSegmentReclaimability for DefaultReclaimabilityPolicy {
    fn can_reclaim_segment(&self, segment: &WalGcCandidate) -> ReclaimabilityDecision {
        // Rule 1: Check recovery boundary (earliest)
        if segment.creation_lsn <= self.boundaries.required_recovery_lsn {
            return ReclaimabilityDecision::BlockedByRecovery {
                segment_start_lsn: segment.creation_lsn,
                required_recovery_lsn: self.boundaries.required_recovery_lsn,
            };
        }

        // Rule 2: Check visibility boundary
        if segment.sealing_lsn >= self.boundaries.min_active_snapshot_lsn {
            return ReclaimabilityDecision::BlockedByVisibility {
                segment_end_lsn: segment.sealing_lsn,
                min_active_snapshot_lsn: self.boundaries.min_active_snapshot_lsn,
            };
        }

        // Rule 3: Check replication boundary
        if segment.sealing_lsn >= self.boundaries.min_standby_received_lsn {
            return ReclaimabilityDecision::BlockedByReplication {
                segment_end_lsn: segment.sealing_lsn,
                min_standby_received_lsn: self.boundaries.min_standby_received_lsn,
            };
        }

        // Rule 4: Check PITR retention window
        if segment.sealing_lsn >= self.boundaries.pitr_retention_lsn {
            return ReclaimabilityDecision::BlockedByPitrRetention {
                segment_end_lsn: segment.sealing_lsn,
                pitr_retention_lsn: self.boundaries.pitr_retention_lsn,
            };
        }

        // All checks passed
        ReclaimabilityDecision::Reclaimable
    }

    fn retention_boundaries(&self) -> RetentionBoundaryPolicy {
        self.boundaries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reclaimability_decision_reason_messages() {
        assert_eq!(
            ReclaimabilityDecision::Reclaimable.reason(),
            "segment is outside all retention boundaries and can be safely garbage-collected"
        );

        let blocked_recovery = ReclaimabilityDecision::BlockedByRecovery {
            segment_start_lsn: Lsn::new(100),
            required_recovery_lsn: Lsn::new(200),
        };
        assert!(!blocked_recovery.is_reclaimable());
        assert!(blocked_recovery.reason().contains("recovery"));

        let blocked_visibility = ReclaimabilityDecision::BlockedByVisibility {
            segment_end_lsn: Lsn::new(300),
            min_active_snapshot_lsn: Lsn::new(300),
        };
        assert!(blocked_visibility.reason().contains("snapshot"));

        let blocked_replication = ReclaimabilityDecision::BlockedByReplication {
            segment_end_lsn: Lsn::new(400),
            min_standby_received_lsn: Lsn::new(500),
        };
        assert!(blocked_replication.reason().contains("standbys"));

        let blocked_pitr = ReclaimabilityDecision::BlockedByPitrRetention {
            segment_end_lsn: Lsn::new(600),
            pitr_retention_lsn: Lsn::new(700),
        };
        assert!(blocked_pitr.reason().contains("point-in-time recovery"));
    }

    #[test]
    fn retention_boundary_policy_validates_lsn_ordering() {
        // Valid: recovery < snapshot. Standby and PITR boundaries are independent.
        let policy = RetentionBoundaryPolicy::new(
            Lsn::new(100),
            Lsn::new(200),
            Lsn::new(150),
            Lsn::new(300),
        );
        assert!(policy.is_ok());

        // Invalid: recovery > snapshot
        let err = RetentionBoundaryPolicy::new(
            Lsn::new(300),
            Lsn::new(200),
            Lsn::new(150),
            Lsn::new(400),
        );
        assert!(err.is_err());

        // Valid: snapshot may be ahead of PITR.
        let policy = RetentionBoundaryPolicy::new(
            Lsn::new(100),
            Lsn::new(400),
            Lsn::new(150),
            Lsn::new(300),
        );
        assert!(policy.is_ok());
    }

    #[test]
    fn retention_boundary_policy_gc_boundary_is_minimum() {
        let policy = RetentionBoundaryPolicy::new(
            Lsn::new(100),
            Lsn::new(300),
            Lsn::new(250), // lowest
            Lsn::new(500),
        )
        .unwrap();

        assert_eq!(policy.gc_boundary_lsn(), Lsn::new(250));
    }

    #[test]
    fn default_policy_reclaimable_segment() {
        let policy = DefaultReclaimabilityPolicy::with_lsns(
            Lsn::new(100), // recovery
            Lsn::new(400), // snapshot
            Lsn::new(350), // standby
            Lsn::new(500), // pitr
        )
        .unwrap();

        // Segment [101, 349] is reclaimable (after recovery, before snapshot and standby)
        let candidate = WalGcCandidate::new(1, Lsn::new(101), Lsn::new(349), 65536).unwrap();
        assert_eq!(
            policy.can_reclaim_segment(&candidate),
            ReclaimabilityDecision::Reclaimable
        );
    }

    #[test]
    fn default_policy_blocked_by_recovery() {
        let policy = DefaultReclaimabilityPolicy::with_lsns(
            Lsn::new(200),
            Lsn::new(400),
            Lsn::new(350),
            Lsn::new(500),
        )
        .unwrap();

        // Segment [100, 199] starts before recovery boundary
        let candidate = WalGcCandidate::new(1, Lsn::new(100), Lsn::new(199), 65536).unwrap();
        assert!(matches!(
            policy.can_reclaim_segment(&candidate),
            ReclaimabilityDecision::BlockedByRecovery { .. }
        ));
    }

    #[test]
    fn default_policy_blocked_by_visibility() {
        let policy = DefaultReclaimabilityPolicy::with_lsns(
            Lsn::new(100),
            Lsn::new(300),
            Lsn::new(350),
            Lsn::new(500),
        )
        .unwrap();

        // Segment [200, 350] overlaps snapshot boundary at 300
        let candidate = WalGcCandidate::new(1, Lsn::new(200), Lsn::new(350), 65536).unwrap();
        assert!(matches!(
            policy.can_reclaim_segment(&candidate),
            ReclaimabilityDecision::BlockedByVisibility { .. }
        ));
    }

    #[test]
    fn default_policy_blocked_by_replication() {
        let policy = DefaultReclaimabilityPolicy::with_lsns(
            Lsn::new(100),
            Lsn::new(400),
            Lsn::new(250), // standby boundary (lowest)
            Lsn::new(500),
        )
        .unwrap();

        // Segment [200, 300] is before snapshot but after standby received
        let candidate = WalGcCandidate::new(1, Lsn::new(200), Lsn::new(300), 65536).unwrap();
        // Actually this should be blocked by replication since 300 >= 250
        assert!(matches!(
            policy.can_reclaim_segment(&candidate),
            ReclaimabilityDecision::BlockedByReplication { .. }
        ));
    }

    #[test]
    fn default_policy_blocks_by_visibility_before_pitr() {
        let policy = DefaultReclaimabilityPolicy::with_lsns(
            Lsn::new(100),
            Lsn::new(300),
            Lsn::new(350),
            Lsn::new(400), // PITR window end
        )
        .unwrap();

        // Segment [200, 350] overlaps the active snapshot boundary first.
        // With the validated ordering snapshot <= PITR, visibility is the
        // earlier blocker for this segment.
        let candidate = WalGcCandidate::new(1, Lsn::new(200), Lsn::new(350), 65536).unwrap();
        assert!(matches!(
            policy.can_reclaim_segment(&candidate),
            ReclaimabilityDecision::BlockedByVisibility { .. }
        ));
    }

    #[test]
    fn reclaimability_decision_blocking_lsn() {
        let decision = ReclaimabilityDecision::BlockedByRecovery {
            segment_start_lsn: Lsn::new(100),
            required_recovery_lsn: Lsn::new(200),
        };
        assert_eq!(decision.blocking_lsn(), Some(Lsn::new(200)));

        assert_eq!(ReclaimabilityDecision::Reclaimable.blocking_lsn(), None);
    }

    #[test]
    fn retention_boundary_validate_passes_for_valid_policy() {
        let policy = RetentionBoundaryPolicy::new(
            Lsn::new(100),
            Lsn::new(200),
            Lsn::new(150),
            Lsn::new(300),
        )
        .unwrap();
        assert!(policy.validate().is_ok());
    }
}
