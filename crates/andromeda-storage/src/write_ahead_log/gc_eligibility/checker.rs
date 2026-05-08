use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::Lsn;

use super::EligibilityResult;

/// WAL segment garbage collection eligibility checker.
///
/// This struct encapsulates the eligibility decision logic. It does not perform
/// removal or archive verification; those are handled by the WAL GC subsystem.
///
/// # Construction
///
/// ```ignore
/// let checker = GcEligibilityChecker::new(
///     segment_lsn_start,
///     segment_lsn_end,
///     min_active_snapshot_lsn,
///     required_recovery_lsn,
/// )?;
/// ```
///
/// All parameters are validated on construction to ensure LSN ordering invariants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcEligibilityChecker {
    /// First LSN in this segment
    segment_lsn_start: Lsn,
    /// Last LSN in this segment (inclusive)
    segment_lsn_end: Lsn,
    /// Oldest active snapshot visibility boundary LSN
    min_active_snapshot_lsn: Lsn,
    /// Oldest LSN required by crash recovery (from manifest)
    required_recovery_lsn: Lsn,
}

impl GcEligibilityChecker {
    /// Create a new GC eligibility checker for a WAL segment.
    ///
    /// # Arguments
    ///
    /// - `segment_lsn_start`: First LSN in the segment (must not be zero)
    /// - `segment_lsn_end`: Last LSN in the segment (must be >= start)
    /// - `min_active_snapshot_lsn`: Minimum LSN of all active snapshots
    /// - `required_recovery_lsn`: From manifest (minimum LSN needed for recovery)
    ///
    /// # Validation
    ///
    /// - `segment_lsn_start` must not be zero
    /// - `segment_lsn_end` must not be zero
    /// - `segment_lsn_start <= segment_lsn_end` (segment contains data)
    /// - `required_recovery_lsn <= min_active_snapshot_lsn` (recovery is older than snapshots)
    ///
    /// # Returns
    ///
    /// - `Ok(checker)` on success
    /// - `Err` if any invariant is violated
    pub fn new(
        segment_lsn_start: Lsn,
        segment_lsn_end: Lsn,
        min_active_snapshot_lsn: Lsn,
        required_recovery_lsn: Lsn,
    ) -> AndromedaResult<Self> {
        // Validate segment LSN range
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

        // Validate global LSN ordering: recovery must be older than active snapshots
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
    ///
    /// A segment is eligible if:
    /// 1. Its end LSN is lower than the minimum active snapshot LSN (visibility rule)
    /// 2. Its start LSN is greater than the required recovery LSN (recovery rule)
    ///
    /// # Returns
    ///
    /// - `EligibilityResult::Eligible` if both rules are satisfied
    /// - `EligibilityResult::BlockedByVisibility` if the segment is not older than the visibility boundary
    /// - `EligibilityResult::BlockedByRecovery` if the segment is needed for recovery
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Segment in the safe zone (older than active snapshots, newer than recovery)
    /// let checker = GcEligibilityChecker::new(
    ///     Lsn::new(101), Lsn::new(199),  // segment
    ///     Lsn::new(200),                 // min snapshot LSN
    ///     Lsn::new(100),                 // recovery LSN
    /// )?;
    /// assert_eq!(checker.is_eligible(), EligibilityResult::Eligible);
    ///
    /// // Segment containing the active snapshot boundary
    /// let checker = GcEligibilityChecker::new(
    ///     Lsn::new(150), Lsn::new(200),  // segment
    ///     Lsn::new(200),                 // min snapshot LSN
    ///     Lsn::new(100),                 // recovery LSN
    /// )?;
    /// assert!(!checker.is_eligible().is_eligible());
    /// ```
    pub fn is_eligible(&self) -> EligibilityResult {
        // Rule 1: only segments wholly before the oldest active snapshot boundary
        // are eligible for GC.
        if self.segment_lsn_end >= self.min_active_snapshot_lsn {
            return EligibilityResult::BlockedByVisibility {
                segment_end_lsn: self.segment_lsn_end,
                min_active_snapshot_lsn: self.min_active_snapshot_lsn,
            };
        }

        // Rule 2: Check recovery boundary (recovery doesn't need this segment)
        if self.segment_lsn_start <= self.required_recovery_lsn {
            return EligibilityResult::BlockedByRecovery {
                segment_start_lsn: self.segment_lsn_start,
                required_recovery_lsn: self.required_recovery_lsn,
            };
        }

        // Both rules satisfied
        EligibilityResult::Eligible
    }

    /// Get the segment's start LSN
    pub const fn segment_start(&self) -> Lsn {
        self.segment_lsn_start
    }

    /// Get the segment's end LSN
    pub const fn segment_end(&self) -> Lsn {
        self.segment_lsn_end
    }

    /// Get the current minimum active snapshot LSN
    pub const fn min_active_snapshot_lsn(&self) -> Lsn {
        self.min_active_snapshot_lsn
    }

    /// Get the required recovery LSN from manifest
    pub const fn required_recovery_lsn(&self) -> Lsn {
        self.required_recovery_lsn
    }

    /// Validate all internal invariants.
    ///
    /// This is primarily for debugging and can be called periodically to detect
    /// corruption or inconsistency in the eligibility state.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if all invariants are satisfied
    /// - `Err` if any invariant is violated (indicates engine bug)
    pub fn validate(&self) -> AndromedaResult<()> {
        // Segment LSN range
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

        // Global LSN ordering
        if self.required_recovery_lsn > self.min_active_snapshot_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Internal,
                "required recovery LSN exceeds min active snapshot LSN after construction",
            ));
        }

        Ok(())
    }

    /// Estimate the gap between this segment and the eligibility boundary.
    ///
    /// Returns the number of LSNs (approximately) that must be produced before
    /// this segment becomes eligible. Returns 0 if already eligible.
    ///
    /// This is useful for diagnostics and GC scheduling decisions.
    pub fn lsn_distance_to_eligibility(&self) -> u64 {
        if self.is_eligible().is_eligible() {
            return 0;
        }

        // If blocked by visibility, report how far the active snapshot boundary must move
        // forward to be strictly past this segment.
        if self.segment_lsn_end >= self.min_active_snapshot_lsn {
            return self.segment_lsn_end.get() - self.min_active_snapshot_lsn.get() + 1;
        }

        // If blocked by recovery, gap is from recovery LSN to start of segment
        if self.segment_lsn_start <= self.required_recovery_lsn {
            return self.required_recovery_lsn.get() - self.segment_lsn_start.get() + 1;
        }

        // Shouldn't reach here if is_eligible() said we're ineligible
        0
    }
}
