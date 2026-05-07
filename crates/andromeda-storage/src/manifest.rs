use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion};
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
    pub const fn recovery_floor_lsn(self) -> Lsn {
        self.required_wal_start_lsn
    }

    pub const fn checkpoint_lsn(self) -> Lsn {
        self.base_checkpoint_lsn
    }

    pub const fn can_start_recovery_at(self, lsn: Lsn) -> bool {
        lsn.get() >= self.required_wal_start_lsn.get()
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.database_id == 0 || self.manifest_version == 0 || self.snapshot_id == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "manifest identity fields must not be zero",
            ));
        }

        if self.required_wal_start_lsn < self.base_checkpoint_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "manifest required WAL start LSN must not precede checkpoint LSN",
            ));
        }

        if self.manifest_crc == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "manifest CRC must not be zero",
            ));
        }

        Ok(())
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
