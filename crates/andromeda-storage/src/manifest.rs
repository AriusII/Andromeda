use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion};
pub use andromeda_manifest::ManifestDurabilityBoundary;
use andromeda_observe::{ManifestEventKind, ManifestTrace, TraceId};

use crate::Lsn;

mod cold_publication;
mod format;
mod hash;
mod snapshot;

pub use cold_publication::{
    ColdSegmentPublicationPlan, validate_cold_segment_publication_boundary,
};
pub use format::{DATABASE_MANIFEST_STORAGE_FORMAT_FINGERPRINTS, StorageFormatManifest};
pub use snapshot::{
    DatabaseSnapshotPublication, SnapshotAvailabilityContract, SnapshotSegmentReference,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatabaseManifest {
    pub database_id: u64,
    pub manifest_version: u64,
    pub snapshot_id: u64,
    pub base_checkpoint_lsn: Lsn,
    pub required_wal_start_lsn: Lsn,
    pub previous_manifest_hash: [u8; 32],
    pub manifest_crc: u32,
}

impl DatabaseManifest {
    pub const fn durability_boundary(self) -> ManifestDurabilityBoundary {
        ManifestDurabilityBoundary {
            database_id: self.database_id,
            manifest_version: self.manifest_version,
            snapshot_id: self.snapshot_id,
            base_checkpoint_lsn: self.base_checkpoint_lsn,
            required_wal_start_lsn: self.required_wal_start_lsn,
            previous_manifest_hash: self.previous_manifest_hash,
            manifest_crc: self.manifest_crc,
        }
    }

    pub const fn recovery_floor_lsn(self) -> Lsn {
        self.durability_boundary().recovery_floor_lsn()
    }

    pub const fn checkpoint_lsn(self) -> Lsn {
        self.durability_boundary().checkpoint_lsn()
    }

    pub const fn can_start_recovery_at(self, lsn: Lsn) -> bool {
        self.durability_boundary().can_start_recovery_at(lsn)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.durability_boundary().validate()
    }

    pub fn storage_format_manifest(&self) -> AndromedaResult<StorageFormatManifest> {
        StorageFormatManifest::from_database_manifest(self)
    }

    pub fn validation_trace(
        &self,
        trace_id: TraceId,
        catalog_version: CatalogVersion,
        manifest_epoch: u64,
    ) -> ManifestTrace {
        ManifestTrace {
            trace_id,
            event: ManifestEventKind::Validation,
            catalog_version,
            manifest_epoch,
            base_checkpoint_lsn: self.base_checkpoint_lsn.get(),
            required_wal_start_lsn: self.required_wal_start_lsn.get(),
            accepted: self.validate().is_ok(),
            reason: "manifest validation checked identity, CRC, and WAL recovery floor".to_string(),
        }
    }
}

pub(super) fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests;
