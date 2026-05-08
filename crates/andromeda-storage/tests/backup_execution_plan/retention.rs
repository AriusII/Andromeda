use crate::support::*;
use andromeda_storage::{
    BackupId, ExtentState, FileBackedBackupArtifactStore, Lsn,
    write_ahead_log::{
        DefaultReclaimabilityPolicy, ReclaimabilityDecision, WalGcCandidate,
        WalSegmentReclaimability,
    },
};

#[test]
fn file_backed_artifact_store_refuses_to_overwrite_existing_backup_id() {
    let temp = tempfile::TempDir::new().unwrap();
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();

    let snapshot_bytes = b"immutable snapshot artifact";
    let wal_bytes = b"immutable wal artifact";
    let manifest = test_manifest(71, 101, 200);
    let extent = test_extent(1, 1000, 1, ExtentState::PublishedCold);
    let wal_segment = test_wal_segment(10, 101, 200, None);

    let plan = backup_plan(
        manifest,
        vec![cold_extent_copy_task(extent, snapshot_bytes.len() as u64)],
        vec![wal_segment_copy_task(
            wal_segment,
            wal_bytes.len() as u64,
            0,
        )],
        true,
        snapshot_bytes.len() as u64,
        wal_bytes.len() as u64,
    );

    store
        .write_execution_plan_artifact(&plan, snapshot_bytes, &[wal_bytes.as_slice()])
        .expect("initial backup artifact write should succeed");

    let err = store
        .write_execution_plan_artifact(&plan, snapshot_bytes, &[wal_bytes.as_slice()])
        .expect_err("backup artifact directory must be immutable for a completed backup id");

    assert!(
        err.message().contains("refusing overwrite"),
        "unexpected error: {err}"
    );

    let record = store
        .validate_artifact_directory(BackupId::new(71))
        .expect("original backup artifact must remain valid after refused overwrite");
    assert_eq!(record.manifest, manifest);
}

#[test]
fn pitr_retention_boundary_blocks_wal_segment_reclaim_until_window_moves() {
    let segment = WalGcCandidate::new(91, Lsn::new(600), Lsn::new(700), 65_536).unwrap();
    let protected = DefaultReclaimabilityPolicy::with_lsns(
        Lsn::new(100),
        Lsn::new(1_000),
        Lsn::new(1_000),
        Lsn::new(700),
    )
    .unwrap();

    let decision = protected.can_reclaim_segment(&segment);
    assert!(matches!(
        decision,
        ReclaimabilityDecision::BlockedByPitrRetention {
            segment_end_lsn,
            pitr_retention_lsn,
        } if segment_end_lsn == Lsn::new(700) && pitr_retention_lsn == Lsn::new(700)
    ));
    assert!(!decision.is_reclaimable());
    assert_eq!(decision.blocking_lsn(), Some(Lsn::new(700)));

    let expired = DefaultReclaimabilityPolicy::with_lsns(
        Lsn::new(100),
        Lsn::new(1_000),
        Lsn::new(1_000),
        Lsn::new(701),
    )
    .unwrap();

    assert_eq!(
        expired.can_reclaim_segment(&segment),
        ReclaimabilityDecision::Reclaimable
    );
}
