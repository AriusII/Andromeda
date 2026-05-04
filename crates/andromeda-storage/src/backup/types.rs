//! Primitive identifiers and durable boundary types for backup metadata.
//!
//! All types here are structural identity only — no RAM-derived state, no GPU
//! fields, no runtime handles.

use andromeda_core::AndromedaResult;

use crate::DatabaseManifest;
use crate::Lsn;

use super::helpers::backup_error;

pub const BACKUP_PHYSICAL_PLAN_VERSION_V0: u16 = 1;
pub const BACKUP_SUPPORTED_STORAGE_FORMAT_VERSION_V0: u16 = 1;

/// Stable identifier of a backup artifact.
///
/// The numeric id is opaque; equality is by `(database_id, backup_id)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BackupId(u64);

impl BackupId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

/// Inclusive LSN range covered by a durable WAL archive that ships alongside
/// a cold snapshot. Both endpoints are inclusive and `start <= end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalArchiveRange {
    pub start: Lsn,
    pub end_inclusive: Lsn,
}

impl WalArchiveRange {
    pub const fn new(start: Lsn, end_inclusive: Lsn) -> Self {
        Self {
            start,
            end_inclusive,
        }
    }

    pub fn validate(self) -> AndromedaResult<()> {
        if self.start.is_zero() {
            return Err(backup_error("WAL archive start LSN must not be zero"));
        }
        if self.end_inclusive < self.start {
            return Err(backup_error(
                "WAL archive end LSN must not precede start LSN",
            ));
        }
        Ok(())
    }

    /// True if `lsn` falls within `[start, end_inclusive]`.
    pub fn contains(self, lsn: Lsn) -> bool {
        lsn >= self.start && lsn <= self.end_inclusive
    }
}

/// Cold snapshot boundary referenced by a backup manifest.
///
/// Mirrors the durable identity in [`DatabaseManifest`] without depending on
/// any RAM-resident derived state. `base_checkpoint_lsn` is the highest LSN
/// included in the snapshot itself; `required_wal_start_lsn` is the first
/// LSN the accompanying WAL archive must cover for replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColdSnapshotBoundary {
    pub snapshot_id: u64,
    pub snapshot_descriptor_hash: [u8; 32],
    pub base_checkpoint_lsn: Lsn,
    pub required_wal_start_lsn: Lsn,
}

impl ColdSnapshotBoundary {
    pub fn from_manifest(manifest: &DatabaseManifest, snapshot_descriptor_hash: [u8; 32]) -> Self {
        Self {
            snapshot_id: manifest.snapshot_id,
            snapshot_descriptor_hash,
            base_checkpoint_lsn: manifest.base_checkpoint_lsn,
            required_wal_start_lsn: manifest.required_wal_start_lsn,
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.snapshot_id == 0 {
            return Err(backup_error("snapshot id must not be zero"));
        }
        if self.snapshot_descriptor_hash == [0; 32] {
            return Err(backup_error("snapshot descriptor hash must not be zero"));
        }
        if self.required_wal_start_lsn < self.base_checkpoint_lsn {
            return Err(backup_error(
                "snapshot required WAL start LSN must not precede base checkpoint LSN",
            ));
        }
        Ok(())
    }
}
