use crate::support::*;
use andromeda_segment::ExtentState;

#[test]
fn test_backup_artifact_metadata_consistency() {
    let manifest = test_manifest(3, 101, 300);
    let extent = test_extent(1, 1000, 100, ExtentState::PublishedCold);
    let wal_seg = test_wal_segment(1, 101, 300, None);

    let true_extent_bytes = 100 * 4096;
    let true_wal_bytes = 200 * 1024;

    let plan = backup_plan(
        manifest,
        vec![cold_extent_copy_task(extent, true_extent_bytes)],
        vec![wal_segment_copy_task(wal_seg, true_wal_bytes, 0)],
        false,
        true_extent_bytes,
        true_wal_bytes,
    );

    assert!(
        plan.validate().is_ok(),
        "backup plan with consistent metadata should pass validation"
    );

    let corrupted_plan = andromeda_backup::BackupExecutionPlan {
        total_extent_bytes: true_extent_bytes + 1000,
        ..plan.clone()
    };

    assert!(
        corrupted_plan.validate().is_err(),
        "backup plan with corrupted extent bytes should fail validation"
    );

    let corrupted_plan = andromeda_backup::BackupExecutionPlan {
        total_wal_bytes: true_wal_bytes - 1000,
        ..plan.clone()
    };

    assert!(
        corrupted_plan.validate().is_err(),
        "backup plan with corrupted WAL bytes should fail validation"
    );

    assert_eq!(
        plan.total_extent_bytes, true_extent_bytes,
        "plan extent bytes should match true value"
    );
    assert_eq!(
        plan.total_wal_bytes, true_wal_bytes,
        "plan WAL bytes should match true value"
    );

    let total = plan.total_bytes().unwrap();
    assert_eq!(
        total,
        true_extent_bytes + true_wal_bytes,
        "plan total bytes should sum extent and WAL"
    );

    let extent2 = test_extent(2, 1100, 50, ExtentState::PublishedCold);
    let mut extra_extent_plan = plan.clone();
    extra_extent_plan
        .extent_copy_plan
        .push(cold_extent_copy_task(extent2, 50 * 4096));
    extra_extent_plan.total_extent_bytes = true_extent_bytes;

    assert!(
        extra_extent_plan.validate().is_err(),
        "backup plan with extent count mismatch should fail validation"
    );
}
