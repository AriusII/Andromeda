#![forbid(unsafe_code)]
#![doc = r#"
Boundary crate for Andromeda manifests and root pointers.

This crate owns manifest durability boundary contracts that are independent of
storage page and segment implementation. Storage keeps the current manifest
surface during the migration and delegates boundary validation here.

C5 invariants:

- Manifest switches must prove durable WAL coverage for checkpoint and recovery-floor evidence.
- Manifest bytes must be versioned and explicitly encoded.
- Persistent and network bytes must use explicit codecs, never Rust native struct layout.
- Crash/recovery validation is required before mission-critical behavior lands here.
- RAM, temporary storage, GPU output, and benchmark output are advisory only; they are not truth.
"#]

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_wal::Lsn;

pub mod codec;
mod cold_publication;
mod database;
mod format;
pub mod format_version;
pub mod root_pointer;
mod snapshot;
mod storage_format_hash;
pub mod trace_event;

pub use codec::{
    MANIFEST_ENCODED_SIZE, MANIFEST_FORMAT_VERSION, MANIFEST_RECORD_MAGIC, crc32c, decode_manifest,
    encode_manifest,
};
pub use cold_publication::PublishedColdSegment;
pub use database::DatabaseManifest;
pub use format::{DATABASE_MANIFEST_STORAGE_FORMAT_FINGERPRINTS, StorageFormatManifest};
pub use format_version::{
    CompatibilityMatrix, CompatibilityResult, FormatVersion, StorageFormatFingerprint,
    StorageFormatKind,
};
pub use root_pointer::{
    ROOT_POINTER_ENCODED_SIZE, ROOT_POINTER_FILE_A, ROOT_POINTER_FILE_B, ROOT_POINTER_MAGIC,
    RootPointerRecord, read_root_pointer, select_valid_pointer, write_root_pointer,
};
pub use snapshot::{
    DatabaseSnapshotPublication, SnapshotAvailabilityContract, SnapshotSegmentReference,
};
pub use storage_format_hash::{ManifestFormatHashInput, storage_format_manifest_hash};
pub use trace_event::{ManifestSwitchEvent, ManifestSwitchTracer};

/// Subdirectory within the database root where the dual-copy manifest root
/// pointer files (`manifest_root_a`, `manifest_root_b`) are stored.
///
/// ## Startup read convention (P05 — W4)
///
/// On every startup the recovery pipeline MUST read exactly these files in
/// order, before the WAL scan, to satisfy the exit criterion:
///
/// > *"Startup lit root + manifest + SegmentIndex + WAL tail, pas tout le ColdStore."*
///
/// ```text
/// {db_dir}/{MANIFEST_ROOT_DIR}/manifest_root_a   (ROOT_POINTER_FILE_A)
/// {db_dir}/{MANIFEST_ROOT_DIR}/manifest_root_b   (ROOT_POINTER_FILE_B)
/// ```
///
/// The winning pointer (highest generation, valid CRC32C) resolves the active
/// `DatabaseManifest`. The manifest's `segment_index_file_id` field then
/// locates the `SegmentIndexV0` file, which must be decoded before the WAL
/// scan begins.
pub const MANIFEST_ROOT_DIR: &str = "manifest";

/// Durable recovery root fields carried by an accepted database manifest.
///
/// This is a domain boundary, not a disk codec. Persistent manifest bytes still
/// require explicit little-endian encoding, version fields, and checksums before
/// promotion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManifestDurabilityBoundary {
    /// Stable database identity — must be non-zero.
    pub database_id: u64,
    /// Monotonically increasing generation counter.
    pub manifest_version: u64,
    /// Cold-snapshot anchor.
    pub snapshot_id: u64,
    /// Last completed WAL checkpoint LSN.
    pub base_checkpoint_lsn: Lsn,
    /// Minimum WAL start LSN for recovery.
    pub required_wal_start_lsn: Lsn,
    /// Hash of the previous manifest (hash chain back-pointer).
    pub previous_manifest_hash: [u8; 32],
    /// Domain CRC — must be non-zero.
    pub manifest_crc: u32,
}

impl ManifestDurabilityBoundary {
    /// Recovery floor LSN.
    #[must_use]
    pub const fn recovery_floor_lsn(self) -> Lsn {
        self.required_wal_start_lsn
    }

    /// Last completed checkpoint LSN.
    #[must_use]
    pub const fn checkpoint_lsn(self) -> Lsn {
        self.base_checkpoint_lsn
    }

    /// Returns `true` if recovery can start at `lsn`.
    #[must_use]
    pub const fn can_start_recovery_at(self, lsn: Lsn) -> bool {
        lsn.get() >= self.required_wal_start_lsn.get()
    }

    /// Validate identity fields, LSN ordering, and CRC non-zero invariants.
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.database_id == 0 || self.manifest_version == 0 || self.snapshot_id == 0 {
            return Err(manifest_error("manifest identity fields must not be zero"));
        }

        if self.required_wal_start_lsn < self.base_checkpoint_lsn {
            return Err(manifest_error(
                "manifest required WAL start LSN must not precede checkpoint LSN",
            ));
        }

        if self.manifest_crc == 0 {
            return Err(manifest_error("manifest CRC must not be zero"));
        }

        Ok(())
    }
}

fn manifest_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

/// Validate that a manifest checkpoint can safely be persisted.
///
/// This fence must pass before a `ManifestSwitch` WAL record is written.
pub fn validate_manifest_atomic_switch(
    manifest_checkpoint_lsn: Lsn,
    wal_durable_lsn: Lsn,
    wal_checkpoint_lsn: Lsn,
) -> AndromedaResult<()> {
    if manifest_checkpoint_lsn.is_zero()
        && (!wal_checkpoint_lsn.is_zero() || !wal_durable_lsn.is_zero())
    {
        return Err(manifest_error(
            "bootstrap manifest checkpoint conflicts with nonzero WAL evidence",
        ));
    }

    if wal_checkpoint_lsn.get() > wal_durable_lsn.get() {
        return Err(manifest_error(
            "WAL checkpoint LSN exceeds durable WAL LSN; manifest switch is not safe",
        ));
    }

    if manifest_checkpoint_lsn.get() > wal_checkpoint_lsn.get() {
        return Err(manifest_error(
            "manifest checkpoint LSN exceeds WAL checkpoint LSN; manifest switch is not safe",
        ));
    }

    Ok(())
}

/// Validate that recovery can start from a given durable floor.
pub fn validate_recovery_floor(
    recovery_floor_lsn: Lsn,
    required_wal_start_lsn: Lsn,
) -> AndromedaResult<()> {
    if recovery_floor_lsn.get() < required_wal_start_lsn.get() {
        return Err(manifest_error(
            "recovery floor LSN is before manifest required WAL start LSN",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_storage_error<T>(result: AndromedaResult<T>) {
        match result {
            Err(error) => assert_eq!(error.kind(), AndromedaErrorKind::Storage),
            Ok(_) => panic!("expected storage error"),
        }
    }

    fn boundary() -> ManifestDurabilityBoundary {
        ManifestDurabilityBoundary {
            database_id: 1,
            manifest_version: 2,
            snapshot_id: 3,
            base_checkpoint_lsn: Lsn::new(10),
            required_wal_start_lsn: Lsn::new(10),
            previous_manifest_hash: [4; 32],
            manifest_crc: 5,
        }
    }

    #[test]
    fn manifest_boundary_validates_identity_crc_and_recovery_floor() {
        assert!(boundary().validate().is_ok());

        let mut invalid = boundary();
        invalid.manifest_crc = 0;
        assert_storage_error(invalid.validate());

        let mut invalid = boundary();
        invalid.required_wal_start_lsn = Lsn::new(9);
        assert_storage_error(invalid.validate());
    }

    #[test]
    fn manifest_switch_and_recovery_fences_validate() {
        assert!(
            validate_manifest_atomic_switch(Lsn::new(500), Lsn::new(600), Lsn::new(500)).is_ok()
        );
        assert!(validate_recovery_floor(Lsn::new(300), Lsn::new(300)).is_ok());

        assert!(
            validate_manifest_atomic_switch(Lsn::new(600), Lsn::new(600), Lsn::new(500)).is_err()
        );
        assert!(validate_recovery_floor(Lsn::new(200), Lsn::new(300)).is_err());
    }
}

#[cfg(test)]
mod primitive_tests;
