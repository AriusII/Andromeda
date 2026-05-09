use crate::support::*;
use andromeda_backup::{BackupId, FileBackedBackupArtifactStore};
use andromeda_segment::ExtentState;

#[test]
fn test_backup_plan_validates_manifest() {
    let manifest = test_manifest(1, 101, 300);

    let extent1 = test_extent(1, 1000, 100, ExtentState::PublishedCold);
    let extent2 = test_extent(2, 1100, 50, ExtentState::PublishedCold);

    let wal_seg1 = test_wal_segment(1, 101, 200, None);
    let wal_seg2 = test_wal_segment(2, 201, 300, Some(200));

    let extent1_bytes = 100 * 16 * 1024;
    let extent2_bytes = 50 * 16 * 1024;
    let wal1_bytes = 100 * 1024;
    let wal2_bytes = 100 * 1024;

    let plan = backup_plan(
        manifest,
        vec![
            cold_extent_copy_task(extent1, extent1_bytes),
            cold_extent_copy_task(extent2, extent2_bytes),
        ],
        vec![
            wal_segment_copy_task(wal_seg1, wal1_bytes, 0),
            wal_segment_copy_task(wal_seg2, wal2_bytes, 1),
        ],
        true,
        extent1_bytes + extent2_bytes,
        wal1_bytes + wal2_bytes,
    );

    assert!(
        plan.validate().is_ok(),
        "valid backup execution plan should pass validation"
    );

    let bad_manifest = BackupManifest {
        backup_id: BackupId::new(0),
        ..manifest
    };

    let bad_plan = BackupExecutionPlan {
        manifest: bad_manifest,
        ..plan.clone()
    };

    assert!(
        bad_plan.validate().is_err(),
        "backup plan with zero backup_id should fail validation"
    );

    let bad_manifest = BackupManifest {
        manifest_crc: 0,
        ..manifest
    };

    let bad_plan = BackupExecutionPlan {
        manifest: bad_manifest,
        ..plan.clone()
    };

    assert!(
        bad_plan.validate().is_err(),
        "backup plan with zero manifest_crc should fail validation"
    );

    let mutable_extent = test_extent(3, 1200, 25, ExtentState::AllocatingHot);

    let bad_plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![cold_extent_copy_task(mutable_extent, 25 * 4096)],
        ..plan
    };

    assert!(
        bad_plan.validate().is_err(),
        "backup plan with mutable extent should fail validation"
    );
}

#[test]
fn file_backed_artifact_store_rejects_manifest_wal_segment_count_above_bound() {
    let temp = temp_dir();
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();

    let snapshot_bytes = b"snapshot bytes with oversized manifest count";
    let wal_bytes = b"durable wal segment";
    let manifest = test_manifest(62, 101, 200);
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
    rewrite_manifest_wal_segment_count(&report.manifest_path, 16_385);

    let err = store
        .validate_artifact_directory(BackupId::new(62))
        .unwrap_err();
    assert!(
        err.message().contains("bounded manifest segment limit"),
        "unexpected error: {err}"
    );
}

#[test]
fn file_backed_artifact_store_rejects_cold_snapshot_manifest_version_mismatch() {
    let temp = temp_dir();
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();

    let snapshot_bytes = b"snapshot bytes with manifest version mismatch";
    let wal_bytes = b"durable wal segment";
    let manifest = test_manifest(63, 101, 200);
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
    rewrite_cold_snapshot_manifest_version(&report.manifest_path, manifest.created_epoch + 1);

    let err = store
        .validate_artifact_directory(BackupId::new(63))
        .unwrap_err();
    assert!(
        err.message().contains("manifest version"),
        "unexpected error: {err}"
    );
}

#[test]
fn file_backed_artifact_store_rejects_cold_snapshot_crc_mismatch() {
    let temp = temp_dir();
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();

    let snapshot_bytes = b"snapshot bytes with cold snapshot crc mismatch";
    let wal_bytes = b"durable wal segment";
    let manifest = test_manifest(59, 101, 200);
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
    rewrite_cold_snapshot_manifest_crc(&report.manifest_path, manifest.manifest_crc + 1);

    let err = store
        .validate_artifact_directory(BackupId::new(59))
        .unwrap_err();
    assert!(
        err.message().contains("manifest CRC"),
        "unexpected error: {err}"
    );
}
