use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::Lsn;

/// Result of eligibility checking for a WAL segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EligibilityResult {
    /// Segment is safe for garbage collection.
    Eligible,
    /// Segment is not wholly older than the oldest active snapshot visibility boundary.
    BlockedByVisibility {
        segment_end_lsn: Lsn,
        min_active_snapshot_lsn: Lsn,
    },
    /// Segment is required by crash recovery.
    BlockedByRecovery {
        segment_start_lsn: Lsn,
        required_recovery_lsn: Lsn,
    },
}

impl EligibilityResult {
    /// Check if the result indicates eligibility.
    pub fn is_eligible(self) -> bool {
        matches!(self, Self::Eligible)
    }

    /// Produce a human-readable description of the reason.
    pub fn reason(self) -> &'static str {
        match self {
            Self::Eligible => {
                "segment is older than the oldest active snapshot and is not needed for recovery"
            },
            Self::BlockedByVisibility { .. } => {
                "segment is not older than the oldest active snapshot visibility boundary"
            },
            Self::BlockedByRecovery { .. } => "segment is required for crash recovery",
        }
    }
}

/// WAL segment garbage collection eligibility checker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcEligibilityChecker {
    segment_lsn_start: Lsn,
    segment_lsn_end: Lsn,
    min_active_snapshot_lsn: Lsn,
    required_recovery_lsn: Lsn,
}

impl GcEligibilityChecker {
    /// Create a new GC eligibility checker for a WAL segment.
    pub fn new(
        segment_lsn_start: Lsn,
        segment_lsn_end: Lsn,
        min_active_snapshot_lsn: Lsn,
        required_recovery_lsn: Lsn,
    ) -> AndromedaResult<Self> {
        if segment_lsn_start.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL segment start LSN must not be zero",
            ));
        }

        if segment_lsn_end.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL segment end LSN must not be zero",
            ));
        }

        if segment_lsn_start > segment_lsn_end {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL segment start LSN must not exceed end LSN",
            ));
        }

        if required_recovery_lsn > min_active_snapshot_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "required recovery LSN must not exceed minimum active snapshot LSN",
            ));
        }

        Ok(Self {
            segment_lsn_start,
            segment_lsn_end,
            min_active_snapshot_lsn,
            required_recovery_lsn,
        })
    }

    /// Check whether this segment is eligible for garbage collection.
    pub fn is_eligible(&self) -> EligibilityResult {
        if self.segment_lsn_end >= self.min_active_snapshot_lsn {
            return EligibilityResult::BlockedByVisibility {
                segment_end_lsn: self.segment_lsn_end,
                min_active_snapshot_lsn: self.min_active_snapshot_lsn,
            };
        }

        if self.segment_lsn_start <= self.required_recovery_lsn {
            return EligibilityResult::BlockedByRecovery {
                segment_start_lsn: self.segment_lsn_start,
                required_recovery_lsn: self.required_recovery_lsn,
            };
        }

        EligibilityResult::Eligible
    }

    /// Get the segment's start LSN.
    pub const fn segment_start(&self) -> Lsn {
        self.segment_lsn_start
    }

    /// Get the segment's end LSN.
    pub const fn segment_end(&self) -> Lsn {
        self.segment_lsn_end
    }

    /// Get the current minimum active snapshot LSN.
    pub const fn min_active_snapshot_lsn(&self) -> Lsn {
        self.min_active_snapshot_lsn
    }

    /// Get the required recovery LSN from the manifest.
    pub const fn required_recovery_lsn(&self) -> Lsn {
        self.required_recovery_lsn
    }

    /// Validate all internal invariants.
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.segment_lsn_start.is_zero() || self.segment_lsn_end.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Internal,
                "segment LSN values must not be zero after construction",
            ));
        }

        if self.segment_lsn_start > self.segment_lsn_end {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Internal,
                "segment start LSN exceeds end LSN after construction",
            ));
        }

        if self.required_recovery_lsn > self.min_active_snapshot_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Internal,
                "required recovery LSN exceeds min active snapshot LSN after construction",
            ));
        }

        Ok(())
    }

    /// Estimate how far the segment is from becoming eligible.
    pub fn lsn_distance_to_eligibility(&self) -> u64 {
        if self.is_eligible().is_eligible() {
            return 0;
        }

        if self.segment_lsn_end >= self.min_active_snapshot_lsn {
            return self.segment_lsn_end.get() - self.min_active_snapshot_lsn.get() + 1;
        }

        if self.segment_lsn_start <= self.required_recovery_lsn {
            return self.required_recovery_lsn.get() - self.segment_lsn_start.get() + 1;
        }

        0
    }
}

#[cfg(test)]
mod tests;
