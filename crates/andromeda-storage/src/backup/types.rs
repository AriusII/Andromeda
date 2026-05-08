//! Compatibility facade for backup primitive DTOs.

use crate::{DatabaseManifest, Lsn};

pub use andromeda_backup::{
    BACKUP_PHYSICAL_PLAN_VERSION_V0, BACKUP_SUPPORTED_STORAGE_FORMAT_VERSION_V0, BackupId,
    ColdSnapshotBoundary as ColdSnapshotBoundaryRaw, WalArchiveRange as WalArchiveRangeRaw,
};

pub type WalArchiveRange = WalArchiveRangeRaw<Lsn>;
pub type ColdSnapshotBoundary = ColdSnapshotBoundaryRaw<Lsn>;

pub fn cold_snapshot_boundary_from_manifest(
    manifest: &DatabaseManifest,
    snapshot_descriptor_hash: [u8; 32],
) -> ColdSnapshotBoundary {
    ColdSnapshotBoundary {
        snapshot_id: manifest.snapshot_id,
        snapshot_descriptor_hash,
        base_checkpoint_lsn: manifest.base_checkpoint_lsn,
        required_wal_start_lsn: manifest.required_wal_start_lsn,
    }
}
