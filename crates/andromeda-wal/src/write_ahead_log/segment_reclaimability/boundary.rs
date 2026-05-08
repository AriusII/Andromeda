use crate::Lsn;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// All LSN retention boundaries for WAL segment GC policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionBoundaryPolicy {
    /// Crash recovery floor from the durable recovery manifest.
    pub required_recovery_lsn: Lsn,
    /// Oldest active snapshot LSN from the snapshot registry.
    pub min_active_snapshot_lsn: Lsn,
    /// Minimum LSN durably received by all configured standbys.
    pub min_standby_received_lsn: Lsn,
    /// PITR retention boundary from backup policy.
    pub pitr_retention_lsn: Lsn,
}

impl RetentionBoundaryPolicy {
    /// Create a new retention policy with validated boundary ordering.
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
