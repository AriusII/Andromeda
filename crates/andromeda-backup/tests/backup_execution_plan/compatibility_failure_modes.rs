use crate::support::*;
use andromeda_backup::{BackupId, FileBackedBackupArtifactStore};
use andromeda_segment::ExtentState;

#[test]
fn file_backed_artifact_store_rejects_incompatible_v3_wal_format_evidence() {
    let temp = temp_dir();
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();

    let snapshot_bytes = b"snapshot bytes with compatibility corruption";
    let wal_bytes = b"durable wal segment";
    let manifest = test_manifest(61, 101, 200);
    let extent = test_extent(1, 1000, 1, ExtentState::PublishedCold);
    let wal_seg = test_wal_segment(10, 101, 200, None);

    let plan = backup_plan(
        manifest,
        vec![cold_extent_copy_task(extent, snapshot_bytes.len() as u64)],
        vec![wal_segment_copy_task(wal_seg, wal_bytes.len() as u64, 0)],
        true,
        snapshot_bytes.len() as u64,
        wal_bytes.len() as u64,
    );

    let report = store
        .write_execution_plan_artifact(&plan, snapshot_bytes, &[wal_bytes.as_slice()])
        .unwrap();
    rewrite_compatibility_wal_format(&report.manifest_path, 999);

    let err = store
        .validate_artifact_directory(BackupId::new(61))
        .unwrap_err();
    assert!(
        err.message().contains("WAL format"),
        "unexpected error: {err}"
    );
}

#[test]
fn file_backed_artifact_store_reads_legacy_v2_manifest_without_compatibility_evidence() {
    let temp = temp_dir();
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();

    let snapshot_bytes = b"legacy v2 snapshot bytes for backup artifact";
    let wal1_bytes = b"legacy v2 wal segment one";
    let wal2_bytes = b"legacy v2 wal segment two";
    let manifest = test_manifest(60, 101, 300);
    let extent = test_extent(1, 1000, 1, ExtentState::PublishedCold);
    let wal_seg1 = test_wal_segment(10, 101, 200, None);
    let wal_seg2 = test_wal_segment(11, 201, 300, Some(200));

    let plan = backup_plan(
        manifest,
        vec![cold_extent_copy_task(extent, snapshot_bytes.len() as u64)],
        vec![
            wal_segment_copy_task(wal_seg1, wal1_bytes.len() as u64, 0),
            wal_segment_copy_task(wal_seg2, wal2_bytes.len() as u64, 1),
        ],
        true,
        snapshot_bytes.len() as u64,
        (wal1_bytes.len() + wal2_bytes.len()) as u64,
    );

    let report = store
        .write_execution_plan_artifact(
            &plan,
            snapshot_bytes,
            &[wal1_bytes.as_slice(), wal2_bytes.as_slice()],
        )
        .unwrap();
    rewrite_manifest_to_v2_without_compatibility_evidence(&report.manifest_path);

    let record = store
        .validate_artifact_directory(BackupId::new(60))
        .unwrap();
    assert_eq!(
        record.manifest_format_version,
        LEGACY_V2_ARTIFACT_MANIFEST_FORMAT_VERSION
    );
    assert!(!record.compatibility_evidence.recorded_in_manifest);
    assert_eq!(
        record.compatibility_evidence.manifest_format_version,
        LEGACY_V2_ARTIFACT_MANIFEST_FORMAT_VERSION
    );
    assert_eq!(record.compatibility_evidence.storage_format_version, 1);
    assert_eq!(record.compatibility_evidence.wal_format_version, 1);
    assert_eq!(
        record.wal_archive_evidence.archive_digest_sha256,
        report.wal_archive_evidence.archive_digest_sha256
    );
}

#[test]
fn file_backed_artifact_store_reads_legacy_v1_manifest_with_reconstructed_wal_digest() {
    let temp = temp_dir();
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();

    let snapshot_bytes = b"legacy snapshot bytes for backup artifact";
    let wal1_bytes = b"legacy wal segment one";
    let wal2_bytes = b"legacy wal segment two";
    let manifest = test_manifest(57, 101, 300);
    let extent = test_extent(1, 1000, 1, ExtentState::PublishedCold);
    let wal_seg1 = test_wal_segment(10, 101, 200, None);
    let wal_seg2 = test_wal_segment(11, 201, 300, Some(200));

    let plan = backup_plan(
        manifest,
        vec![cold_extent_copy_task(extent, snapshot_bytes.len() as u64)],
        vec![
            wal_segment_copy_task(wal_seg1, wal1_bytes.len() as u64, 0),
            wal_segment_copy_task(wal_seg2, wal2_bytes.len() as u64, 1),
        ],
        true,
        snapshot_bytes.len() as u64,
        (wal1_bytes.len() + wal2_bytes.len()) as u64,
    );

    let report = store
        .write_execution_plan_artifact(
            &plan,
            snapshot_bytes,
            &[wal1_bytes.as_slice(), wal2_bytes.as_slice()],
        )
        .unwrap();
    rewrite_manifest_to_v1_without_archive_digest(&report.manifest_path);

    let record = store
        .validate_artifact_directory(BackupId::new(57))
        .unwrap();
    assert_eq!(record.manifest_format_version, 1);
    assert!(!record.compatibility_evidence.recorded_in_manifest);
    assert_eq!(
        record.compatibility_evidence.manifest_format_version,
        LEGACY_V1_ARTIFACT_MANIFEST_FORMAT_VERSION
    );
    assert_eq!(
        record.wal_archive_evidence.archive_digest_sha256,
        report.wal_archive_evidence.archive_digest_sha256
    );
}

#[test]
fn file_backed_artifact_store_rejects_corrupted_snapshot() {
    let temp = temp_dir();
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();

    let snapshot_bytes = b"snapshot before corruption";
    let wal_bytes = b"durable wal segment";
    let manifest = test_manifest(56, 101, 200);
    let extent = test_extent(1, 1000, 1, ExtentState::PublishedCold);
    let wal_seg = test_wal_segment(10, 101, 200, None);

    let plan = backup_plan(
        manifest,
        vec![cold_extent_copy_task(extent, snapshot_bytes.len() as u64)],
        vec![wal_segment_copy_task(wal_seg, wal_bytes.len() as u64, 0)],
        true,
        snapshot_bytes.len() as u64,
        wal_bytes.len() as u64,
    );

    let report = store
        .write_execution_plan_artifact(&plan, snapshot_bytes, &[wal_bytes.as_slice()])
        .unwrap();
    let mut corrupted_snapshot = snapshot_bytes.to_vec();
    corrupted_snapshot[0] ^= 0xFF;
    std::fs::write(&report.snapshot_path, corrupted_snapshot).unwrap();

    let err = store
        .validate_artifact_directory(BackupId::new(56))
        .unwrap_err();
    assert!(
        err.message()
            .contains("snapshot artifact checksum mismatch"),
        "unexpected error: {err}"
    );
}
