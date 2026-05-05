//! WAL Segment Garbage Collection Eligibility Model
//!
//! # Overview
//!
//! This module implements the eligibility checking logic for WAL segment garbage collection.
//! It provides a clean separation between eligibility **determination** (this module) and
//! eligibility **enforcement** (archive verification, segment removal).
//!
//! # Eligibility Rules
//!
//! A WAL segment is eligible for garbage collection if and only if:
//!
//! 1. **Visibility Rule**: The segment is older than the oldest active snapshot
//!    - Segment is blocked when `segment_end_lsn >= min_active_snapshot_lsn`
//!    - Only segments wholly before the oldest active snapshot boundary are eligible.
//!
//! 2. **Recovery Rule**: The segment is not required by recovery
//!    - Segment start LSN > required recovery LSN (from manifest)
//!    - This ensures recovery doesn't need this segment to rebuild state
//!
//! Both rules must be satisfied for a segment to be eligible.
//!
//! # Architecture
//!
//! ```ignore
//! GcEligibilityChecker
//!   ├── check_eligibility(candidate) -> EligibilityResult
//!   │   ├── ✓ Eligible (safe to remove)
//!   │   ├── ✗ Blocked by visibility (still needed by snapshots)
//!   │   └── ✗ Blocked by recovery (needed for crash recovery)
//!   ├── validate() -> AndromedaResult<()> (invariant checks)
//!   └── (no removal logic - that's in gc.rs)
//! ```
//!
//! # Invariants Maintained
//!
//! ## Critical Invariants (violations = engine bugs)
//!
//! 1. **No Snapshot Boundary Loss**
//!    - If a segment contains `min_active_snapshot_lsn`, it is ineligible
//!    - Ensures the oldest active snapshot boundary is never removed
//!
//! 2. **No Recovery Data Loss**
//!    - If segment_start_lsn <= required_recovery_lsn, segment is ineligible
//!    - Ensures recovery can always rebuild from manifest boundary
//!
//! 3. **LSN Ordering**
//!    - segment_lsn_start < segment_lsn_end (segment contains data)
//!    - required_recovery_lsn <= min_active_snapshot_lsn (recovery is not newer than snapshots)
//!    - These are enforced on construction
//!
//! ## Recovery Boundary Safety
//!
//! The `required_recovery_lsn` from the manifest represents:
//! - The checkpoint LSN (or earliest undo record required)
//! - The oldest WAL record needed to rebuild state after crash
//! - **Never** eligible for GC: segments starting at or before this LSN
//!
//! Example:
//! ```ignore
//! Manifest: checkpoint_lsn=100, required_recovery_lsn=50
//! Active snapshots: min_active_snapshot_lsn=200
//!
//! Segment A: [1, 40]    -> INELIGIBLE (needed for recovery, < 50)
//! Segment B: [50, 99]   -> INELIGIBLE (overlaps recovery boundary at 50)
//! Segment C: [100, 150] -> ELIGIBLE by visibility, subject to archive checks
//! Segment D: [200, 250] -> INELIGIBLE (contains min active snapshot LSN)
//! Segment E: [251, 350] -> INELIGIBLE (newer than min active snapshot LSN)
//! ```
//!
//! # Proof of Invariant Preservation
//!
//! **Theorem**: If all eligible segments are removed, the oldest active snapshot
//! boundary remains observable and recovery always has required records.
//!
//! **Proof**:
//! 1. For any segment S marked eligible: `S.end < min_active_snapshot_lsn`
//! 2. For any segment S marked eligible: `S.start > required_recovery_lsn`
//! 3. By (1): The oldest active snapshot boundary record is retained
//! 4. By (2): Recovery has all records before S (recovery starts at required_recovery_lsn)
//! 5. Therefore: Removing S preserves the checked snapshot boundary and recovery capability ∎
//!
//! # No Unsafe Code
//!
//! This module must forbid(unsafe_code).

use crate::Lsn;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_rejects_zero_start_lsn() {
        let result =
            GcEligibilityChecker::new(Lsn::new(0), Lsn::new(100), Lsn::new(200), Lsn::new(50));
        assert!(result.is_err());
    }

    #[test]
    fn test_new_rejects_zero_end_lsn() {
        let result =
            GcEligibilityChecker::new(Lsn::new(1), Lsn::new(0), Lsn::new(200), Lsn::new(50));
        assert!(result.is_err());
    }

    #[test]
    fn test_new_rejects_start_exceeds_end() {
        let result =
            GcEligibilityChecker::new(Lsn::new(100), Lsn::new(50), Lsn::new(200), Lsn::new(25));
        assert!(result.is_err());
    }

    #[test]
    fn test_new_rejects_recovery_exceeds_snapshot() {
        let result =
            GcEligibilityChecker::new(Lsn::new(1), Lsn::new(100), Lsn::new(150), Lsn::new(200));
        assert!(result.is_err());
    }

    #[test]
    fn test_eligible_segment_in_safe_zone() {
        let checker =
            GcEligibilityChecker::new(Lsn::new(101), Lsn::new(199), Lsn::new(200), Lsn::new(100))
                .unwrap();

        assert_eq!(checker.is_eligible(), EligibilityResult::Eligible);
    }

    #[test]
    fn test_visibility_boundary_is_strictly_after_segment_end() {
        let checker =
            GcEligibilityChecker::new(Lsn::new(101), Lsn::new(199), Lsn::new(200), Lsn::new(100))
                .unwrap();

        // The active snapshot boundary is just past the segment end, so the
        // segment is wholly older than every active snapshot.
        assert_eq!(checker.is_eligible(), EligibilityResult::Eligible);
    }

    #[test]
    fn test_blocked_by_visibility_end_at_boundary() {
        let checker =
            GcEligibilityChecker::new(Lsn::new(150), Lsn::new(200), Lsn::new(200), Lsn::new(100))
                .unwrap();

        let result = checker.is_eligible();
        assert!(matches!(
            result,
            EligibilityResult::BlockedByVisibility { .. }
        ));
    }

    #[test]
    fn test_blocked_by_visibility_when_segment_is_newer_than_snapshot() {
        let checker =
            GcEligibilityChecker::new(Lsn::new(201), Lsn::new(250), Lsn::new(200), Lsn::new(100))
                .unwrap();

        // A segment that begins after the oldest active snapshot is newer than
        // that snapshot's visibility boundary and must not be reclaimed.
        assert_eq!(
            checker.is_eligible(),
            EligibilityResult::BlockedByVisibility {
                segment_end_lsn: Lsn::new(250),
                min_active_snapshot_lsn: Lsn::new(200),
            }
        );
    }

    #[test]
    fn test_blocked_by_visibility_segment_overlaps_snapshot() {
        let checker =
            GcEligibilityChecker::new(Lsn::new(150), Lsn::new(250), Lsn::new(200), Lsn::new(50))
                .unwrap();

        assert!(matches!(
            checker.is_eligible(),
            EligibilityResult::BlockedByVisibility { .. }
        ));
    }

    #[test]
    fn test_blocked_by_recovery_start_at_recovery_boundary() {
        let checker =
            GcEligibilityChecker::new(Lsn::new(100), Lsn::new(199), Lsn::new(200), Lsn::new(100))
                .unwrap();

        assert!(matches!(
            checker.is_eligible(),
            EligibilityResult::BlockedByRecovery { .. }
        ));
    }

    #[test]
    fn test_blocked_by_recovery_segment_before_recovery() {
        let checker =
            GcEligibilityChecker::new(Lsn::new(50), Lsn::new(99), Lsn::new(300), Lsn::new(100))
                .unwrap();

        assert!(matches!(
            checker.is_eligible(),
            EligibilityResult::BlockedByRecovery { .. }
        ));
    }

    #[test]
    fn test_both_blocked_by_visibility_first() {
        // Blocked by both rules, but visibility check comes first
        let checker =
            GcEligibilityChecker::new(Lsn::new(100), Lsn::new(200), Lsn::new(200), Lsn::new(100))
                .unwrap();

        // The segment contains min_snapshot (200), and segment_start (100) <= recovery (100),
        // so both predicates block. Visibility is reported first for deterministic observability.
        let result = checker.is_eligible();
        assert!(matches!(
            result,
            EligibilityResult::BlockedByVisibility { .. }
        ));
    }

    #[test]
    fn test_validate_passes_for_valid_checker() {
        let checker =
            GcEligibilityChecker::new(Lsn::new(101), Lsn::new(199), Lsn::new(200), Lsn::new(100))
                .unwrap();

        assert!(checker.validate().is_ok());
    }

    #[test]
    fn test_lsn_distance_zero_when_eligible() {
        let checker =
            GcEligibilityChecker::new(Lsn::new(101), Lsn::new(199), Lsn::new(200), Lsn::new(100))
                .unwrap();

        assert_eq!(checker.lsn_distance_to_eligibility(), 0);
    }

    #[test]
    fn test_lsn_distance_zero_when_segment_before_snapshot_boundary() {
        let checker =
            GcEligibilityChecker::new(Lsn::new(150), Lsn::new(180), Lsn::new(200), Lsn::new(50))
                .unwrap();

        let distance = checker.lsn_distance_to_eligibility();
        // Segment is wholly before the active snapshot boundary and is not blocked by recovery.
        assert_eq!(distance, 0);
    }

    #[test]
    fn test_lsn_distance_blocked_by_visibility_correct() {
        let checker =
            GcEligibilityChecker::new(Lsn::new(150), Lsn::new(205), Lsn::new(200), Lsn::new(50))
                .unwrap();

        // min_snapshot (200) is inside [150, 205], so visibility blocks until the
        // snapshot boundary advances past 205.
        assert_eq!(
            checker.is_eligible(),
            EligibilityResult::BlockedByVisibility {
                segment_end_lsn: Lsn::new(205),
                min_active_snapshot_lsn: Lsn::new(200),
            }
        );
        assert_eq!(checker.lsn_distance_to_eligibility(), 6);
    }

    #[test]
    fn test_lsn_distance_when_segment_end_equals_snapshot() {
        let checker =
            GcEligibilityChecker::new(Lsn::new(150), Lsn::new(200), Lsn::new(200), Lsn::new(50))
                .unwrap();

        // min_snapshot (200) is the segment end boundary, so the segment overlaps visibility.
        assert!(matches!(
            checker.is_eligible(),
            EligibilityResult::BlockedByVisibility { .. }
        ));
    }

    #[test]
    fn test_segment_accessors() {
        let checker =
            GcEligibilityChecker::new(Lsn::new(300), Lsn::new(399), Lsn::new(200), Lsn::new(100))
                .unwrap();

        assert_eq!(checker.segment_start(), Lsn::new(300));
        assert_eq!(checker.segment_end(), Lsn::new(399));
        assert_eq!(checker.min_active_snapshot_lsn(), Lsn::new(200));
        assert_eq!(checker.required_recovery_lsn(), Lsn::new(100));
    }

    #[test]
    fn test_eligibility_result_reason() {
        assert_eq!(
            EligibilityResult::Eligible.reason(),
            "segment is older than the oldest active snapshot and is not needed for recovery"
        );
    }

    #[test]
    fn test_multiple_segments_cascade() {
        // Simulate a cascade of segments with recovery boundary at 100,
        // min snapshot at 300
        let ineligible_recovery =
            GcEligibilityChecker::new(Lsn::new(1), Lsn::new(99), Lsn::new(300), Lsn::new(100))
                .unwrap();
        assert!(!ineligible_recovery.is_eligible().is_eligible());

        let boundary_recovery =
            GcEligibilityChecker::new(Lsn::new(100), Lsn::new(199), Lsn::new(300), Lsn::new(100))
                .unwrap();
        assert!(!boundary_recovery.is_eligible().is_eligible());

        let ineligible_visibility =
            GcEligibilityChecker::new(Lsn::new(200), Lsn::new(300), Lsn::new(300), Lsn::new(100))
                .unwrap();
        assert!(!ineligible_visibility.is_eligible().is_eligible());

        let eligible =
            GcEligibilityChecker::new(Lsn::new(101), Lsn::new(199), Lsn::new(300), Lsn::new(100))
                .unwrap();
        assert!(eligible.is_eligible().is_eligible());
    }

    #[test]
    fn test_error_kind_on_invalid_construction() {
        let result =
            GcEligibilityChecker::new(Lsn::new(0), Lsn::new(100), Lsn::new(200), Lsn::new(50));
        assert_eq!(result.unwrap_err().kind(), AndromedaErrorKind::Storage);
    }

    #[test]
    fn test_eligibility_result_is_eligible_method() {
        assert!(EligibilityResult::Eligible.is_eligible());
        assert!(
            !EligibilityResult::BlockedByVisibility {
                segment_end_lsn: Lsn::new(100),
                min_active_snapshot_lsn: Lsn::new(200),
            }
            .is_eligible()
        );
        assert!(
            !EligibilityResult::BlockedByRecovery {
                segment_start_lsn: Lsn::new(50),
                required_recovery_lsn: Lsn::new(100),
            }
            .is_eligible()
        );
    }
}
