use crate::support::*;
use andromeda_backup::{BackupId, FileBackedBackupArtifactStore};
use andromeda_segment::ExtentState;

#[test]
fn file_backed_artifact_store_writes_manifest_snapshot_and_wal() {
    let temp = temp_dir();
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();

    let snapshot_bytes = b"durable snapshot bytes for backup artifact";
    let wal1_bytes = b"durable wal segment one";
    let wal2_bytes = b"durable wal segment two";
    let manifest = test_manifest(55, 101, 300);
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
            TEST_CATALOG_BYTES,
            TEST_AUDIT_LEDGER_BYTES,
        )
        .unwrap();

    assert!(report.manifest_path().exists());
    assert!(report.snapshot_path().exists());
    assert!(report.catalog_path().exists());
    assert!(report.audit_ledger_path().exists());
    assert_eq!(report.wal_segment_paths().len(), 2);
    assert_eq!(
        report.source_checkpoint_lsn(),
        manifest.snapshot.base_checkpoint_lsn
    );
    assert_eq!(
        report.wal_archive_evidence().total_bytes,
        (wal1_bytes.len() + wal2_bytes.len()) as u64
    );
    assert_ne!(
        report.wal_archive_evidence().archive_digest_sha256,
        [0; 32],
        "WAL archive evidence must carry an aggregate durable digest"
    );
    assert_eq!(
        report.compatibility_evidence().manifest_format_version,
        CURRENT_ARTIFACT_MANIFEST_FORMAT_VERSION
    );
    assert_eq!(report.compatibility_evidence().physical_plan_version, 1);
    assert_eq!(report.compatibility_evidence().storage_format_version, 1);
    assert_eq!(report.compatibility_evidence().wal_format_version, 1);
    assert!(
        report.compatibility_evidence().recorded_in_manifest,
        "current backup manifests must persist compatibility evidence"
    );

    let reopened = FileBackedBackupArtifactStore::open_existing(temp.path()).unwrap();
    let record = reopened
        .validate_artifact_directory(BackupId::new(55))
        .unwrap();
    assert_eq!(record.manifest, manifest);
    assert_eq!(
        record.manifest_format_version,
        CURRENT_ARTIFACT_MANIFEST_FORMAT_VERSION
    );
    assert_eq!(
        record.compatibility_evidence,
        *report.compatibility_evidence()
    );
    assert_eq!(
        record.artifact_set.cold_snapshot.artifact.byte_len,
        snapshot_bytes.len() as u64
    );
    assert_ne!(record.artifact_set.catalog.catalog_version, 0);
    assert_ne!(record.artifact_set.audit_ledger.ledger_epoch, 0);
    assert_eq!(record.artifact_set.wal_segments.len(), 2);
    assert_eq!(
        record.wal_archive_evidence.archive_digest_sha256,
        report.wal_archive_evidence().archive_digest_sha256
    );
}

#[test]
fn file_backed_artifact_store_strict_write_persists_catalog_and_audit_bytes() {
    let temp = temp_dir();
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();
    let snapshot_bytes = b"snapshot strict bytes";
    let wal_bytes = b"wal strict bytes";
    let catalog_bytes = b"catalog strict bytes";
    let audit_ledger_bytes = b"audit strict bytes";
    let manifest = test_manifest(69, 101, 200);
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
        .write_execution_plan_artifact_strict(
            &plan,
            snapshot_bytes,
            &[wal_bytes.as_slice()],
            catalog_bytes,
            audit_ledger_bytes,
        )
        .unwrap();
    let loaded = store
        .validate_artifact_directory(BackupId::new(69))
        .unwrap();
    assert_eq!(
        loaded.artifact_set.catalog.artifact,
        report.artifact_set().catalog.artifact
    );
    assert_eq!(
        loaded.artifact_set.audit_ledger.artifact,
        report.artifact_set().audit_ledger.artifact
    );
}
