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
//! ```text
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

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::Lsn;

/// Result of eligibility checking for a WAL segment.
///
/// Indicates whether a segment can be garbage collected and why (if ineligible).
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcEligibilityChecker {
    /// First LSN in this segment.
    segment_lsn_start: Lsn,
    /// Last LSN in this segment, inclusive.
    segment_lsn_end: Lsn,
    /// Oldest active snapshot visibility boundary LSN.
    min_active_snapshot_lsn: Lsn,
    /// Oldest LSN required by crash recovery, from the manifest.
    required_recovery_lsn: Lsn,
}

impl GcEligibilityChecker {
    /// Create a new GC eligibility checker for a WAL segment.
    ///
    /// Validation keeps the segment range non-empty and ensures the recovery
    /// floor is not newer than the active snapshot boundary.
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
    ///
    /// Returns zero when already eligible.
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
