//! WAL durability fence for LSN-ordered flush coordination.
//!
//! This module provides validation and tracking for LSN ordering invariants that must
//! hold during:
//! - Dirty page writeback to cold storage
//! - Manifest switches and checkpoints
//! - Crash recovery initialization
//!
//! The key invariant enforced by durability fences:
//! > A page's in-memory modifications can only become durable in cold storage
//! > after the WAL has itself durably persisted those modifications.
//!
//! This is enforced through LSN ordering:
//! - `page.first_dirty_lsn` ≤ `wal.durable_lsn` (required before flush)
//! - `manifest.checkpoint_lsn` ≤ `wal.checkpoint_lsn` (required before manifest switch)
//! - `recovery.recovery_floor_lsn` ≥ `manifest.required_wal_start_lsn` (required for recovery)

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::Lsn;

/// Error type for WAL durability fence violations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DurabilityFenceError {
    /// Page LSN exceeds the WAL's durable LSN (page cannot be safely flushed).
    PageLsnNotDurable { page_lsn: u64, max_durable_lsn: u64 },
    /// Manifest checkpoint LSN exceeds WAL's checkpoint LSN (manifest cannot switch).
    ManifestCheckpointNotDurable {
        manifest_checkpoint_lsn: u64,
        wal_checkpoint_lsn: u64,
    },
    /// Manifest required WAL start LSN exceeds recovery floor (recovery would miss data).
    RecoveryFloorBeforeRequired {
        recovery_floor_lsn: u64,
        required_wal_start_lsn: u64,
    },
    /// LSN ordering violation: one LSN should strictly precede another.
    LsnOrderingViolation {
        earlier_name: String,
        earlier_lsn: u64,
        later_name: String,
        later_lsn: u64,
    },
}

impl DurabilityFenceError {
    fn message(&self) -> String {
        match self {
            Self::PageLsnNotDurable {
                page_lsn,
                max_durable_lsn,
            } => format!(
                "page LSN {} exceeds WAL durable LSN {}; page cannot be safely flushed",
                page_lsn, max_durable_lsn
            ),
            Self::ManifestCheckpointNotDurable {
                manifest_checkpoint_lsn,
                wal_checkpoint_lsn,
            } => format!(
                "manifest checkpoint LSN {} exceeds WAL checkpoint LSN {}; \
                 manifest switch is not safe",
                manifest_checkpoint_lsn, wal_checkpoint_lsn
            ),
            Self::RecoveryFloorBeforeRequired {
                recovery_floor_lsn,
                required_wal_start_lsn,
            } => format!(
                "recovery floor LSN {} is before manifest required WAL start LSN {}; \
                 recovery would miss required data",
                recovery_floor_lsn, required_wal_start_lsn
            ),
            Self::LsnOrderingViolation {
                earlier_name,
                earlier_lsn,
                later_name,
                later_lsn,
            } => format!(
                "LSN ordering violation: {} ({}) should precede {} ({})",
                earlier_name, earlier_lsn, later_name, later_lsn
            ),
        }
    }

    pub fn into_andromeda_error(self) -> AndromedaError {
        AndromedaError::new(AndromedaErrorKind::Storage, self.message())
    }
}

/// Validate that a page's LSN has been made durable in the WAL before page flush.
///
/// **Invariant:** A dirty page can only be written to cold storage if its first
/// modification's LSN is guaranteed durable in the WAL.
///
/// # Arguments
/// - `page_lsn`: The LSN at which this page was first modified (its first_dirty_lsn)
/// - `wal_checkpoint_lsn`: The maximum LSN known to be durable in the WAL
///
/// # Returns
/// - `Ok(())` if page_lsn ≤ wal_checkpoint_lsn (page is safe to flush)
/// - `Err(DurabilityFenceError::PageLsnNotDurable)` otherwise
///
/// # Contract
/// The buffer pool's `flush_dirty_frames()` method calls this implicitly via the
/// `WalDurabilityObserver::is_durable()` check. This function is exported for:
/// - Explicit validation in higher-level APIs
/// - Crash recovery validation
/// - Testing and auditing
///
/// # Example
/// ```ignore
/// use andromeda_storage::{Lsn, validate_wal_durability_before_page_flush};
///
/// let page_lsn = Lsn::new(100);
/// let wal_durable = Lsn::new(100);
///
/// // This page can be safely flushed
/// validate_wal_durability_before_page_flush(page_lsn, wal_durable)?;
/// ```
pub fn validate_wal_durability_before_page_flush(
    page_lsn: Lsn,
    wal_checkpoint_lsn: Lsn,
) -> AndromedaResult<()> {
    if page_lsn.get() > wal_checkpoint_lsn.get() {
        return Err(DurabilityFenceError::PageLsnNotDurable {
            page_lsn: page_lsn.get(),
            max_durable_lsn: wal_checkpoint_lsn.get(),
        }
        .into_andromeda_error());
    }

    Ok(())
}

/// Validate that a manifest checkpoint can safely be persisted.
///
/// **Invariant:** A manifest can only be switched (version bumped) once the WAL has
/// durably persisted all changes referenced by the manifest's new checkpoint.
///
/// # Arguments
/// - `manifest_checkpoint_lsn`: The base checkpoint LSN of the new manifest
/// - `wal_durable_lsn`: The maximum LSN currently durable in the WAL
/// - `wal_checkpoint_lsn`: The checkpoint LSN recorded in the WAL segment
///
/// # Returns
/// - `Ok(())` if manifest_checkpoint_lsn ≤ wal_checkpoint_lsn (manifest is safe to switch)
/// - `Err(DurabilityFenceError::ManifestCheckpointNotDurable)` otherwise
///
/// # Rationale
/// The manifest captures the state of the storage layer at a checkpoint. If we switch
/// to a new manifest version before the WAL has made that checkpoint durable, a crash
/// could leave us in a state where:
/// - The new manifest is persisted (to manifest file)
/// - But the WAL doesn't have the changes that the manifest claims are durable
/// - Recovery would fail because it cannot find required WAL records
///
/// By requiring `manifest_checkpoint_lsn ≤ wal_checkpoint_lsn`, we ensure that:
/// - All changes needed for the new manifest are already in the WAL
/// - The WAL has been flushed at or beyond the checkpoint
/// - Recovery can safely use the new manifest as the starting point
///
/// # Example
/// ```ignore
/// use andromeda_storage::{Lsn, validate_manifest_atomic_switch};
///
/// let manifest_ckpt = Lsn::new(500);
/// let wal_durable = Lsn::new(600);
/// let wal_ckpt = Lsn::new(500);
///
/// // Manifest switch is safe once WAL has reached this checkpoint
/// validate_manifest_atomic_switch(manifest_ckpt, wal_durable, wal_ckpt)?;
/// ```
pub fn validate_manifest_atomic_switch(
    manifest_checkpoint_lsn: Lsn,
    _wal_durable_lsn: Lsn,
    wal_checkpoint_lsn: Lsn,
) -> AndromedaResult<()> {
    if manifest_checkpoint_lsn.get() > wal_checkpoint_lsn.get() {
        return Err(DurabilityFenceError::ManifestCheckpointNotDurable {
            manifest_checkpoint_lsn: manifest_checkpoint_lsn.get(),
            wal_checkpoint_lsn: wal_checkpoint_lsn.get(),
        }
        .into_andromeda_error());
    }

    Ok(())
}

/// Validate that recovery can safely begin from a given floor LSN.
///
/// **Invariant:** Recovery must not miss any changes needed to reconstruct a consistent
/// snapshot. The recovery floor LSN must be at or after the manifest's required WAL
/// start LSN.
///
/// # Arguments
/// - `recovery_floor_lsn`: The earliest LSN where recovery will begin scanning
/// - `required_wal_start_lsn`: The manifest's declared earliest LSN needed for recovery
///
/// # Returns
/// - `Ok(())` if recovery_floor_lsn ≥ required_wal_start_lsn (recovery is safe)
/// - `Err(DurabilityFenceError::RecoveryFloorBeforeRequired)` otherwise
///
/// # Contract
/// This is typically checked at system startup before WAL recovery begins:
/// ```ignore
/// if let Some(manifest) = load_manifest() {
///     validate_recovery_floor(recovery_floor, manifest.required_wal_start_lsn())?;
///     // Now safe to proceed with recovery
/// }
/// ```
pub fn validate_recovery_floor(
    recovery_floor_lsn: Lsn,
    required_wal_start_lsn: Lsn,
) -> AndromedaResult<()> {
    if recovery_floor_lsn.get() < required_wal_start_lsn.get() {
        return Err(DurabilityFenceError::RecoveryFloorBeforeRequired {
            recovery_floor_lsn: recovery_floor_lsn.get(),
            required_wal_start_lsn: required_wal_start_lsn.get(),
        }
        .into_andromeda_error());
    }

    Ok(())
}

/// Validate strict LSN ordering: `earlier < later`.
///
/// Useful for validating state transitions where an LSN must strictly increase.
///
/// # Arguments
/// - `earlier_lsn`: The LSN that should come first
/// - `earlier_name`: Human-readable name for earlier_lsn (e.g., "old checkpoint")
/// - `later_lsn`: The LSN that should come second
/// - `later_name`: Human-readable name for later_lsn (e.g., "new checkpoint")
///
/// # Returns
/// - `Ok(())` if earlier_lsn < later_lsn
/// - `Err(DurabilityFenceError::LsnOrderingViolation)` otherwise
pub fn validate_lsn_strictly_ordered(
    earlier_lsn: Lsn,
    earlier_name: &str,
    later_lsn: Lsn,
    later_name: &str,
) -> AndromedaResult<()> {
    if earlier_lsn.get() >= later_lsn.get() {
        return Err(DurabilityFenceError::LsnOrderingViolation {
            earlier_name: earlier_name.to_string(),
            earlier_lsn: earlier_lsn.get(),
            later_name: later_name.to_string(),
            later_lsn: later_lsn.get(),
        }
        .into_andromeda_error());
    }

    Ok(())
}

/// Validate non-decreasing LSN ordering: `earlier ≤ later`.
///
/// Useful for validating state where LSNs can be equal (e.g., no pages dirtied).
///
/// # Arguments
/// - `earlier_lsn`: The LSN that should come first or be equal
/// - `earlier_name`: Human-readable name for earlier_lsn
/// - `later_lsn`: The LSN that should come second or be equal
/// - `later_name`: Human-readable name for later_lsn
///
/// # Returns
/// - `Ok(())` if earlier_lsn ≤ later_lsn
/// - `Err(DurabilityFenceError::LsnOrderingViolation)` otherwise
pub fn validate_lsn_ordered(
    earlier_lsn: Lsn,
    earlier_name: &str,
    later_lsn: Lsn,
    later_name: &str,
) -> AndromedaResult<()> {
    if earlier_lsn.get() > later_lsn.get() {
        return Err(DurabilityFenceError::LsnOrderingViolation {
            earlier_name: earlier_name.to_string(),
            earlier_lsn: earlier_lsn.get(),
            later_name: later_name.to_string(),
            later_lsn: later_lsn.get(),
        }
        .into_andromeda_error());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_flush_allowed_when_lsn_equals_durable() {
        let page_lsn = Lsn::new(100);
        let wal_durable = Lsn::new(100);
        assert!(validate_wal_durability_before_page_flush(page_lsn, wal_durable).is_ok());
    }

    #[test]
    fn page_flush_allowed_when_lsn_less_than_durable() {
        let page_lsn = Lsn::new(50);
        let wal_durable = Lsn::new(100);
        assert!(validate_wal_durability_before_page_flush(page_lsn, wal_durable).is_ok());
    }

    #[test]
    fn page_flush_blocked_when_lsn_exceeds_durable() {
        let page_lsn = Lsn::new(150);
        let wal_durable = Lsn::new(100);
        let result = validate_wal_durability_before_page_flush(page_lsn, wal_durable);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), AndromedaErrorKind::Storage);
    }

    #[test]
    fn page_flush_allowed_for_zero_lsn() {
        let page_lsn = Lsn::ZERO;
        let wal_durable = Lsn::ZERO;
        assert!(validate_wal_durability_before_page_flush(page_lsn, wal_durable).is_ok());
    }

    #[test]
    fn page_flush_blocked_when_durable_is_zero_and_page_lsn_nonzero() {
        let page_lsn = Lsn::new(1);
        let wal_durable = Lsn::ZERO;
        let result = validate_wal_durability_before_page_flush(page_lsn, wal_durable);
        assert!(result.is_err());
    }

    #[test]
    fn manifest_switch_allowed_when_checkpoint_equals_wal_checkpoint() {
        let manifest_ckpt = Lsn::new(500);
        let wal_durable = Lsn::new(600);
        let wal_ckpt = Lsn::new(500);
        assert!(validate_manifest_atomic_switch(manifest_ckpt, wal_durable, wal_ckpt).is_ok());
    }

    #[test]
    fn manifest_switch_allowed_when_checkpoint_less_than_wal_checkpoint() {
        let manifest_ckpt = Lsn::new(400);
        let wal_durable = Lsn::new(600);
        let wal_ckpt = Lsn::new(500);
        assert!(validate_manifest_atomic_switch(manifest_ckpt, wal_durable, wal_ckpt).is_ok());
    }

    #[test]
    fn manifest_switch_blocked_when_checkpoint_exceeds_wal_checkpoint() {
        let manifest_ckpt = Lsn::new(600);
        let wal_durable = Lsn::new(600);
        let wal_ckpt = Lsn::new(500);
        let result = validate_manifest_atomic_switch(manifest_ckpt, wal_durable, wal_ckpt);
        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("checkpoint LSN"));
    }

    #[test]
    fn manifest_switch_blocked_when_both_checkpoints_zero_but_manifest_higher() {
        // This is vacuous: if both are zero, they're equal, so switch is allowed.
        let manifest_ckpt = Lsn::ZERO;
        let wal_durable = Lsn::ZERO;
        let wal_ckpt = Lsn::ZERO;
        assert!(validate_manifest_atomic_switch(manifest_ckpt, wal_durable, wal_ckpt).is_ok());
    }

    #[test]
    fn recovery_allowed_when_floor_equals_required() {
        let floor = Lsn::new(300);
        let required = Lsn::new(300);
        assert!(validate_recovery_floor(floor, required).is_ok());
    }

    #[test]
    fn recovery_allowed_when_floor_exceeds_required() {
        let floor = Lsn::new(400);
        let required = Lsn::new(300);
        assert!(validate_recovery_floor(floor, required).is_ok());
    }

    #[test]
    fn recovery_blocked_when_floor_before_required() {
        let floor = Lsn::new(200);
        let required = Lsn::new(300);
        let result = validate_recovery_floor(floor, required);
        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("recovery floor"));
    }

    #[test]
    fn recovery_floor_validation_at_system_startup() {
        // Simulate manifest load at startup
        let manifest_required_wal_start = Lsn::new(250);
        let recovery_floor = Lsn::new(250);

        // Should succeed: recovery can begin at the required point
        assert!(validate_recovery_floor(recovery_floor, manifest_required_wal_start).is_ok());
    }

    #[test]
    fn strict_ordering_passes_when_earlier_strictly_less_than_later() {
        let earlier = Lsn::new(100);
        let later = Lsn::new(200);
        assert!(validate_lsn_strictly_ordered(earlier, "old", later, "new").is_ok());
    }

    #[test]
    fn strict_ordering_fails_when_earlier_equals_later() {
        let earlier = Lsn::new(100);
        let later = Lsn::new(100);
        let result = validate_lsn_strictly_ordered(earlier, "old", later, "new");
        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("ordering violation"));
    }

    #[test]
    fn strict_ordering_fails_when_earlier_greater_than_later() {
        let earlier = Lsn::new(200);
        let later = Lsn::new(100);
        let result = validate_lsn_strictly_ordered(earlier, "old", later, "new");
        assert!(result.is_err());
    }

    #[test]
    fn non_decreasing_ordering_passes_when_earlier_less_than_later() {
        let earlier = Lsn::new(100);
        let later = Lsn::new(200);
        assert!(validate_lsn_ordered(earlier, "old", later, "new").is_ok());
    }

    #[test]
    fn non_decreasing_ordering_passes_when_equal() {
        let earlier = Lsn::new(100);
        let later = Lsn::new(100);
        assert!(validate_lsn_ordered(earlier, "old", later, "new").is_ok());
    }

    #[test]
    fn non_decreasing_ordering_fails_when_earlier_greater_than_later() {
        let earlier = Lsn::new(200);
        let later = Lsn::new(100);
        let result = validate_lsn_ordered(earlier, "old", later, "new");
        assert!(result.is_err());
    }

    #[test]
    fn combined_page_flush_then_manifest_switch_is_safe() {
        // Simulate: page dirtied at LSN 100, WAL durable through 200, manifest at 150
        let page_lsn = Lsn::new(100);
        let wal_durable = Lsn::new(200);
        let manifest_ckpt = Lsn::new(150);
        let wal_ckpt = Lsn::new(200);

        // Page can be flushed
        assert!(validate_wal_durability_before_page_flush(page_lsn, wal_durable).is_ok());

        // Manifest can switch
        assert!(validate_manifest_atomic_switch(manifest_ckpt, wal_durable, wal_ckpt).is_ok());
    }

    #[test]
    fn combined_page_flush_blocked_prevents_manifest_switch() {
        // Simulate: page dirtied at LSN 300, WAL durable through 200, manifest at 150
        let page_lsn = Lsn::new(300);
        let wal_durable = Lsn::new(200);
        let manifest_ckpt = Lsn::new(150);
        let wal_ckpt = Lsn::new(200);

        // Page cannot be flushed
        assert!(validate_wal_durability_before_page_flush(page_lsn, wal_durable).is_err());

        // But manifest could theoretically switch (its checkpoint is before wal checkpoint)
        assert!(validate_manifest_atomic_switch(manifest_ckpt, wal_durable, wal_ckpt).is_ok());
    }

    #[test]
    fn recovery_cascade_validation() {
        // Simulate full system checkpoint
        let old_manifest_required = Lsn::new(100);
        let new_manifest_checkpoint = Lsn::new(500);
        let wal_durable = Lsn::new(600);
        let wal_checkpoint = Lsn::new(500);
        let recovery_floor = Lsn::new(500);

        // 1. Check manifest can switch
        assert!(
            validate_manifest_atomic_switch(new_manifest_checkpoint, wal_durable, wal_checkpoint)
                .is_ok()
        );

        // 2. Check recovery can start from the new manifest's required floor
        assert!(validate_recovery_floor(recovery_floor, new_manifest_checkpoint).is_ok());

        // 3. Check ordering: old < new
        assert!(
            validate_lsn_strictly_ordered(
                old_manifest_required,
                "old manifest required",
                new_manifest_checkpoint,
                "new manifest checkpoint"
            )
            .is_ok()
        );
    }

    #[test]
    fn zero_lsn_comparisons_are_consistent() {
        let zero = Lsn::ZERO;
        let one = Lsn::new(1);

        assert!(validate_wal_durability_before_page_flush(zero, zero).is_ok());
        assert!(validate_wal_durability_before_page_flush(zero, one).is_ok());
        assert!(validate_wal_durability_before_page_flush(one, zero).is_err());
    }

    #[test]
    fn max_lsn_comparisons_are_consistent() {
        let max = Lsn::MAX;
        let one_before_max = Lsn::new(u64::MAX - 1);

        assert!(validate_wal_durability_before_page_flush(max, max).is_ok());
        assert!(validate_wal_durability_before_page_flush(one_before_max, max).is_ok());
        assert!(validate_wal_durability_before_page_flush(max, one_before_max).is_err());
    }

    #[test]
    fn error_messages_include_lsn_values() {
        let result = validate_wal_durability_before_page_flush(Lsn::new(150), Lsn::new(100));
        let err = result.unwrap_err();
        let error_msg = err.message();
        assert!(error_msg.contains("150"));
        assert!(error_msg.contains("100"));
    }
}
