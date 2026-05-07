use crate::Lsn;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// All LSN retention boundaries for WAL segment GC policy.
///
/// These boundaries are consulted for every GC candidate to ensure that no
/// segment is reclaimed before it is no longer needed for recovery,
/// visibility, replication, or PITR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionBoundaryPolicy {
    /// Crash recovery floor from the durable recovery manifest.
    ///
    /// Segments with `creation_lsn <= required_recovery_lsn` may still be
    /// needed for replay and must be retained.
    pub required_recovery_lsn: Lsn,

    /// Oldest active snapshot LSN from the snapshot registry.
    ///
    /// Segments with `sealing_lsn >= min_active_snapshot_lsn` may contain data
    /// visible to an active snapshot and must be retained.
    pub min_active_snapshot_lsn: Lsn,

    /// Minimum LSN durably received by all configured standbys.
    ///
    /// Segments with `sealing_lsn > min_standby_received_lsn` have not been
    /// fully replicated and must be retained.
    pub min_standby_received_lsn: Lsn,

    /// PITR retention boundary from backup policy.
    ///
    /// Segments with `sealing_lsn >= pitr_retention_lsn` remain inside the
    /// point-in-time recovery window and must be retained.
    pub pitr_retention_lsn: Lsn,
}

impl RetentionBoundaryPolicy {
    /// Create a new retention policy with validated boundary ordering.
    ///
    /// `required_recovery_lsn` must not exceed `min_active_snapshot_lsn`.
    /// Replica and PITR boundaries are independent because HA lag and PITR
    /// retention can move on different operational clocks.
    pub fn new(
        required_recovery_lsn: Lsn,
        min_active_snapshot_lsn: Lsn,
        min_standby_received_lsn: Lsn,
        pitr_retention_lsn: Lsn,
    ) -> AndromedaResult<Self> {
        validate_retention_boundary_order(required_recovery_lsn, min_active_snapshot_lsn)?;

        Ok(Self {
            required_recovery_lsn,
            min_active_snapshot_lsn,
            min_standby_received_lsn,
            pitr_retention_lsn,
        })
    }

    /// Effective upper GC boundary for segment end-LSN checks.
    ///
    /// The recovery floor is checked separately against segment start LSN, so
    /// it is not folded into this end-LSN ceiling.
    pub fn gc_boundary_lsn(&self) -> Lsn {
        self.min_active_snapshot_lsn
            .min(self.min_standby_received_lsn)
            .min(self.pitr_retention_lsn)
    }

    /// Validate internal consistency of the policy.
    pub fn validate(&self) -> AndromedaResult<()> {
        validate_retention_boundary_order(self.required_recovery_lsn, self.min_active_snapshot_lsn)
    }
}

fn validate_retention_boundary_order(
    required_recovery_lsn: Lsn,
    min_active_snapshot_lsn: Lsn,
) -> AndromedaResult<()> {
    if required_recovery_lsn > min_active_snapshot_lsn {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            "required_recovery_lsn must not exceed min_active_snapshot_lsn",
        ));
    }

    Ok(())
}
