use andromeda_error::AndromedaErrorKind;
use andromeda_observe::{EventCorrelation, EventEnvelope, EventId, TraceEvent, TraceId};
use andromeda_types::CatalogVersion;

use crate::{
    Lsn, SegmentId,
    format_version::{FormatVersion, StorageFormatFingerprint, StorageFormatKind},
};

use super::*;

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
fn manifest_exposes_recovery_floor() {
    let manifest = valid_manifest();

    assert_eq!(manifest.checkpoint_lsn(), Lsn::new(10));
    assert_eq!(manifest.recovery_floor_lsn(), Lsn::new(11));
    assert!(!manifest.can_start_recovery_at(Lsn::new(10)));
    assert!(manifest.can_start_recovery_at(Lsn::new(11)));
}

#[test]
fn manifest_exposes_hashed_storage_format_manifest() {
    let manifest = valid_manifest();
    let storage_manifest = manifest.storage_format_manifest().unwrap();

    assert_eq!(storage_manifest.database_id, manifest.database_id);
    assert_eq!(storage_manifest.manifest_version, manifest.manifest_version);
    assert_eq!(storage_manifest.snapshot_id, manifest.snapshot_id);
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
fn manifest_rejects_wal_start_before_checkpoint() {
    let mut manifest = valid_manifest();
    manifest.required_wal_start_lsn = Lsn::new(9);

    assert_eq!(
        manifest.validate().unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );
}

#[test]
fn manifest_validation_trace_carries_queryable_catalog_and_wal_floor() {
    let manifest = valid_manifest();
    let trace = manifest.validation_trace(TraceId::new(40), CatalogVersion::new(41), 42);

    let envelope = EventEnvelope::new(
        EventId::new(43),
        EventCorrelation {
            catalog_version: Some(CatalogVersion::new(41)),
            ..EventCorrelation::empty()
        },
        TraceEvent::Manifest(trace),
    )
    .expect("manifest validation trace is acceptable forensic evidence");

    assert_eq!(
        envelope.correlation.catalog_version,
        Some(CatalogVersion::new(41))
    );
    assert_eq!(
        match envelope.event {
            TraceEvent::Manifest(trace) => trace.required_wal_start_lsn,
            _ => 0,
        },
        11
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
