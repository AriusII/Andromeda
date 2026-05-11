use crate::support::*;
use andromeda_backup::{BackupId, FileBackedBackupArtifactStore, WalArchiveIntegration};
use andromeda_segment::ExtentState;
use andromeda_wal::Lsn;

#[test]
fn test_backup_plan_includes_wal_range() {
    let wal_start = 1001;
    let wal_end = 3000;

    let manifest = test_manifest(2, wal_start, wal_end);
    let extent = test_extent(1, 1000, 100, ExtentState::PublishedCold);

    let wal_seg1 = test_wal_segment(10, wal_start, 1500, None);
    let wal_seg2 = test_wal_segment(11, 1501, 2500, Some(1500));
    let wal_seg3 = test_wal_segment(12, 2501, wal_end, Some(2500));

    let plan = backup_plan(
        manifest,
        vec![cold_extent_copy_task(extent, 100 * 4096)],
        vec![
            wal_segment_copy_task(wal_seg1, 500 * 1024, 0),
            wal_segment_copy_task(wal_seg2, 1000 * 1024, 1),
            wal_segment_copy_task(wal_seg3, 500 * 1024, 2),
        ],
        false,
        100 * 4096,
        2000 * 1024,
    );

    assert!(
        plan.validate().is_ok(),
        "backup plan with contiguous WAL segments should pass validation"
    );

    assert_eq!(
        plan.wal_segment_count(),
        3,
        "plan should include all 3 WAL segments"
    );

    assert!(
        plan.covers_wal_lsn(Lsn::new(wal_start)),
        "plan should cover WAL start LSN"
    );
    assert!(
        plan.covers_wal_lsn(Lsn::new(2000)),
        "plan should cover LSN in middle of range"
    );
    assert!(
        plan.covers_wal_lsn(Lsn::new(wal_end)),
        "plan should cover WAL end LSN"
    );

    let short_wal_seg = test_wal_segment(20, wal_start, 2000, None);

    let short_plan = backup_plan(
        manifest,
        vec![cold_extent_copy_task(extent, 100 * 4096)],
        vec![wal_segment_copy_task(short_wal_seg, 1000 * 1024, 0)],
        false,
        100 * 4096,
        1000 * 1024,
    );

    assert!(
        short_plan.validate().is_err(),
        "backup plan with WAL segments not covering full range should fail"
    );
}

#[test]
fn test_backup_rejects_incomplete_wal() {
    let manifest = test_manifest(4, 101, 500);
    let extent = test_extent(1, 1000, 100, ExtentState::PublishedCold);

    let wal_seg1 = test_wal_segment(1, 101, 200, None);
    let wal_seg2 = test_wal_segment(2, 300, 500, None);

    let bad_plan_gap = backup_plan(
        manifest,
        vec![cold_extent_copy_task(extent, 100 * 4096)],
        vec![
            wal_segment_copy_task(wal_seg1, 100 * 1024, 0),
            wal_segment_copy_task(wal_seg2, 200 * 1024, 1),
        ],
        false,
        100 * 4096,
        300 * 1024,
    );

    assert!(
        bad_plan_gap.validate().is_err(),
        "backup plan with WAL LSN gap should fail validation"
    );

    let late_start_seg = test_wal_segment(3, 150, 500, None);

    let bad_plan_late_start = backup_plan(
        manifest,
        vec![cold_extent_copy_task(extent, 100 * 4096)],
        vec![wal_segment_copy_task(late_start_seg, 350 * 1024, 0)],
        false,
        100 * 4096,
        350 * 1024,
    );

    assert!(
        bad_plan_late_start.validate().is_err(),
        "backup plan with incorrect WAL start LSN should fail validation"
    );

    let early_end_seg = test_wal_segment(4, 101, 400, None);

    let bad_plan_early_end = backup_plan(
        manifest,
        vec![cold_extent_copy_task(extent, 100 * 4096)],
        vec![wal_segment_copy_task(early_end_seg, 300 * 1024, 0)],
        false,
        100 * 4096,
        300 * 1024,
    );

    assert!(
        bad_plan_early_end.validate().is_err(),
        "backup plan with incorrect WAL end LSN should fail validation"
    );

    let wal_seg_chain1 = test_wal_segment(10, 101, 250, None);
    let wal_seg_chain2 = test_wal_segment(11, 251, 500, Some(249));

    let bad_plan_chain = backup_plan(
        manifest,
        vec![cold_extent_copy_task(extent, 100 * 4096)],
        vec![
            wal_segment_copy_task(wal_seg_chain1, 150 * 1024, 0),
            wal_segment_copy_task(wal_seg_chain2, 250 * 1024, 1),
        ],
        false,
        100 * 4096,
        400 * 1024,
    );

    assert!(
        bad_plan_chain.validate().is_err(),
        "backup plan with WAL segment chain breakage should fail validation"
    );

    let wal_seg_dup1 = test_wal_segment(20, 101, 250, None);
    let wal_seg_dup2 = test_wal_segment(20, 251, 500, Some(250));

    let bad_plan_dup = backup_plan(
        manifest,
        vec![cold_extent_copy_task(extent, 100 * 4096)],
        vec![
            wal_segment_copy_task(wal_seg_dup1, 150 * 1024, 0),
            wal_segment_copy_task(wal_seg_dup2, 250 * 1024, 1),
        ],
        false,
        100 * 4096,
        400 * 1024,
    );

    assert!(
        bad_plan_dup.validate().is_err(),
        "backup plan with duplicate WAL segment IDs should fail validation"
    );

    let wal_seg_good1 = test_wal_segment(30, 101, 250, None);
    let wal_seg_good2 = test_wal_segment(31, 251, 500, Some(250));

    let good_plan = backup_plan(
        manifest,
        vec![cold_extent_copy_task(extent, 100 * 4096)],
        vec![
            wal_segment_copy_task(wal_seg_good1, 150 * 1024, 0),
            wal_segment_copy_task(wal_seg_good2, 250 * 1024, 1),
        ],
        false,
        100 * 4096,
        400 * 1024,
    );

    assert!(
        good_plan.validate().is_ok(),
        "backup plan with complete WAL coverage should pass validation"
    );
}

#[test]
fn backup_plan_rejects_missing_wal_segment_coverage_entirely() {
    let manifest = test_manifest(41, 101, 500);
    let extent = test_extent(1, 1000, 100, ExtentState::PublishedCold);
    let missing_wal_plan = backup_plan(
        manifest,
        vec![cold_extent_copy_task(extent, 100 * 4096)],
        vec![],
        false,
        100 * 4096,
        0,
    );

    let err = missing_wal_plan
        .validate()
        .expect_err("plan must fail when no WAL segment covers the archive");
    assert!(
        err.message()
            .contains("must include at least one WAL segment"),
        "unexpected error: {err}"
    );
}

#[test]
fn wal_archive_target_validation_fails_closed_for_zero_before_and_after_bounds() {
    let manifest = test_manifest(42, 101, 200);

    let zero = WalArchiveIntegration::validate_pitr_target(&manifest, Lsn::new(0)).unwrap_err();
    assert!(
        zero.message().contains("must not be zero"),
        "unexpected error: {zero}"
    );

    let before_base =
        WalArchiveIntegration::validate_pitr_target(&manifest, Lsn::new(99)).unwrap_err();
    assert!(
        before_base
            .message()
            .contains("precedes earliest restorable"),
        "unexpected error: {before_base}"
    );

    let after_archive =
        WalArchiveIntegration::validate_pitr_target(&manifest, Lsn::new(201)).unwrap_err();
    assert!(
        after_archive
            .message()
            .contains("exceeds latest restorable"),
        "unexpected error: {after_archive}"
    );
}

#[test]
fn file_backed_artifact_store_rejects_zero_wal_archive_evidence_digest() {
    let temp = temp_dir();
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();

    let snapshot_bytes = b"snapshot bytes with evidence digest corruption";
    let wal_bytes = b"durable wal segment";
    let manifest = test_manifest(58, 101, 200);
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
        .write_execution_plan_artifact(
            &plan,
            snapshot_bytes,
            &[wal_bytes.as_slice()],
            TEST_CATALOG_BYTES,
            TEST_AUDIT_LEDGER_BYTES,
        )
        .unwrap();
    zero_manifest_archive_digest(report.manifest_path());

    let err = store
        .validate_artifact_directory(BackupId::new(58))
        .unwrap_err();
    assert!(
        err.message().contains("evidence digest"),
        "unexpected error: {err}"
    );
}
