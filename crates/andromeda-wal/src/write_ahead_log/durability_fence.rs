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
//! - `page.latest_dirty_lsn` ≤ `wal.durable_lsn` (required before flush)
//! - `manifest.checkpoint_lsn` ≤ `wal.checkpoint_lsn` ≤ `wal.durable_lsn` (required before manifest switch)
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
    /// WAL checkpoint evidence itself is ahead of the durable WAL prefix.
    WalCheckpointNotDurable {
        wal_checkpoint_lsn: u64,
        wal_durable_lsn: u64,
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
            Self::WalCheckpointNotDurable {
                wal_checkpoint_lsn,
                wal_durable_lsn,
            } => format!(
                "WAL checkpoint LSN {} exceeds durable WAL LSN {}; \
                 manifest switch checkpoint evidence is not durable",
                wal_checkpoint_lsn, wal_durable_lsn
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
/// **Invariant:** A dirty page can only be written to cold storage if the
/// latest dirty LSN represented by the page image is guaranteed durable in the WAL.
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
/// **Invariant:** A manifest can only be switched once the WAL has durably
/// persisted all changes referenced by the manifest's new checkpoint.
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
/// **Invariant:** Recovery must not miss any changes needed to reconstruct a
/// consistent snapshot.
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
mod tests;
