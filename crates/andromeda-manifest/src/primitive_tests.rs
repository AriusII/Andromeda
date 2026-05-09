use andromeda_core::AndromedaErrorKind;
use andromeda_segment::SegmentId;
use andromeda_wal::Lsn;

use crate::{
    CompatibilityMatrix, CompatibilityResult, DATABASE_MANIFEST_STORAGE_FORMAT_FINGERPRINTS,
    DatabaseManifest, DatabaseSnapshotPublication, FormatVersion, SnapshotAvailabilityContract,
    SnapshotSegmentReference, StorageFormatFingerprint, StorageFormatKind, StorageFormatManifest,
};

fn valid_manifest() -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: Lsn::new(10),
        required_wal_start_lsn: Lsn::new(11),
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    }
}

#[test]
fn format_version_rejects_reserved_zero_and_reports_typed_error() {
    let error = FormatVersion::try_new(0, 0).expect_err("zero version must be rejected");
    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert!(error.message().contains("reserved"));

    assert_eq!(
        FormatVersion::try_new(1, 0).expect("v1.0"),
        FormatVersion::V1_0
    );
}

#[test]
fn compatibility_matrix_classifies_reader_writer_versions() {
    let matrix_1_0 = CompatibilityMatrix::new(FormatVersion::V1_0);
    let matrix_1_5 = CompatibilityMatrix::new(FormatVersion::V1_5);

    assert_eq!(
        matrix_1_0.check_compatibility(FormatVersion::V1_0),
        CompatibilityResult::FullyCompatible
    );
    assert_eq!(
        matrix_1_0.check_compatibility(FormatVersion::V1_5),
        CompatibilityResult::Incompatible
    );
    assert_eq!(
        matrix_1_5.check_compatibility(FormatVersion::V1_0),
        CompatibilityResult::BackwardCompatible
    );
}

#[test]
fn database_manifest_exposes_recovery_floor_and_storage_formats() {
    let manifest = valid_manifest();
    let storage_manifest = manifest.storage_format_manifest().unwrap();

    assert_eq!(manifest.checkpoint_lsn(), Lsn::new(10));
    assert_eq!(manifest.recovery_floor_lsn(), Lsn::new(11));
    assert!(manifest.can_start_recovery_at(Lsn::new(11)));
    assert_eq!(
        storage_manifest.fingerprints(),
        DATABASE_MANIFEST_STORAGE_FORMAT_FINGERPRINTS
    );
    assert_ne!(storage_manifest.fingerprint_hash, [0; 32]);
    assert!(storage_manifest.validate().is_ok());
}

#[test]
fn storage_format_manifest_rejects_hash_mismatch_and_duplicates() {
    let manifest = valid_manifest();
    let mut storage_manifest = manifest.storage_format_manifest().unwrap();
    storage_manifest.fingerprints[0].version = FormatVersion::V1_5;

    assert_eq!(
        storage_manifest.validate().unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );

    let duplicate = StorageFormatManifest::new(
        manifest.database_id,
        manifest.manifest_version,
        manifest.snapshot_id,
        vec![
            StorageFormatFingerprint::new(StorageFormatKind::Page, FormatVersion::V1_0),
            StorageFormatFingerprint::new(StorageFormatKind::Page, FormatVersion::V1_0),
        ],
    );
    assert_eq!(
        duplicate.validate().unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );
}

#[test]
fn snapshot_publication_requires_segments_and_matching_manifest() {
    let publication = DatabaseSnapshotPublication {
        manifest: valid_manifest(),
        snapshot_id: 3,
        publication_epoch: 1,
        snapshot_descriptor_hash: [4; 32],
        segments: vec![SnapshotSegmentReference {
            segment_id: SegmentId::new(5),
            descriptor_hash: [6; 32],
        }],
    };
    assert!(publication.validate().is_ok());

    let mut empty = publication.clone();
    empty.segments.clear();
    assert_eq!(
        empty.validate().unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );

    let mut mismatch = publication;
    mismatch.snapshot_id = 9;
    assert_eq!(
        mismatch.validate().unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );
}

#[test]
fn snapshot_availability_requires_one_valid_snapshot_remaining() {
    let contract = SnapshotAvailabilityContract {
        published_snapshot_id: 3,
        available_snapshot_ids: vec![3],
    };
    assert!(contract.validate().is_ok());

    let empty = SnapshotAvailabilityContract {
        published_snapshot_id: 3,
        available_snapshot_ids: Vec::new(),
    };
    assert_eq!(
        empty.validate().unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );

    let missing_published = SnapshotAvailabilityContract {
        published_snapshot_id: 3,
        available_snapshot_ids: vec![4],
    };
    assert_eq!(
        missing_published.validate().unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );
}
