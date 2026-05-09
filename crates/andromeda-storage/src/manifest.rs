use andromeda_error::{AndromedaError, AndromedaErrorKind};
pub use andromeda_manifest::{
    DATABASE_MANIFEST_STORAGE_FORMAT_FINGERPRINTS, DatabaseManifest, DatabaseSnapshotPublication,
    ManifestDurabilityBoundary, SnapshotAvailabilityContract, SnapshotSegmentReference,
    StorageFormatManifest,
};
use andromeda_observe::{ManifestEventKind, ManifestTrace, TraceId};
use andromeda_types::CatalogVersion;

mod cold_publication;

pub use cold_publication::{
    ColdSegmentPublicationPlan, validate_cold_segment_publication_boundary,
};

pub trait DatabaseManifestTraceExt {
    fn validation_trace(
        &self,
        trace_id: TraceId,
        catalog_version: CatalogVersion,
        manifest_epoch: u64,
    ) -> ManifestTrace;
}

impl DatabaseManifestTraceExt for DatabaseManifest {
    fn validation_trace(
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
