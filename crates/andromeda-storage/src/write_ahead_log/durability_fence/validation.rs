use andromeda_core::AndromedaResult;

use crate::Lsn;

use super::DurabilityFenceError;

/// Validate that a page's LSN has been made durable in the WAL before page flush.
///
/// **Invariant:** A dirty page can only be written to cold storage if the
/// latest dirty LSN represented by the page image is guaranteed durable in the WAL.
///
/// # Arguments
/// - `page_lsn`: The highest dirty LSN represented by the page image.
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
/// - `wal_checkpoint_lsn`: The checkpoint LSN recorded in the durable WAL segment
///
/// # Returns
/// - `Ok(())` if manifest_checkpoint_lsn ≤ wal_checkpoint_lsn ≤ wal_durable_lsn
/// - `Err(DurabilityFenceError::ManifestCheckpointNotDurable)` if the manifest
///   outruns the WAL checkpoint
/// - `Err(DurabilityFenceError::WalCheckpointNotDurable)` if checkpoint evidence
///   itself outruns the durable WAL prefix
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
    wal_durable_lsn: Lsn,
    wal_checkpoint_lsn: Lsn,
) -> AndromedaResult<()> {
    if manifest_checkpoint_lsn.is_zero()
        && (!wal_checkpoint_lsn.is_zero() || !wal_durable_lsn.is_zero())
    {
        return Err(DurabilityFenceError::LsnOrderingViolation {
            earlier_name: "bootstrap manifest checkpoint".to_string(),
            earlier_lsn: manifest_checkpoint_lsn.get(),
            later_name: "nonzero durable WAL/checkpoint evidence".to_string(),
            later_lsn: wal_checkpoint_lsn.get().max(wal_durable_lsn.get()),
        }
        .into_andromeda_error());
    }

    if wal_checkpoint_lsn.get() > wal_durable_lsn.get() {
        return Err(DurabilityFenceError::WalCheckpointNotDurable {
            wal_checkpoint_lsn: wal_checkpoint_lsn.get(),
            wal_durable_lsn: wal_durable_lsn.get(),
        }
        .into_andromeda_error());
    }

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
