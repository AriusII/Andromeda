use crate::support::*;
use andromeda_backup::{BackupId, BackupRetentionPolicy, FileBackedBackupArtifactStore};
use andromeda_segment::ExtentState;
use andromeda_wal::Lsn;
use andromeda_wal::write_ahead_log::{
    DefaultReclaimabilityPolicy, ReclaimabilityDecision, WalGcCandidate, WalSegmentReclaimability,
};

#[test]
fn file_backed_artifact_store_refuses_to_overwrite_existing_backup_id() {
    let temp = temp_dir();
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
        .write_execution_plan_artifact(
            &plan,
            snapshot_bytes,
            &[wal_bytes.as_slice()],
            TEST_CATALOG_BYTES,
            TEST_AUDIT_LEDGER_BYTES,
        )
        .expect("initial backup artifact write should succeed");

    let err = store
        .write_execution_plan_artifact(
            &plan,
            snapshot_bytes,
            &[wal_bytes.as_slice()],
            TEST_CATALOG_BYTES,
            TEST_AUDIT_LEDGER_BYTES,
        )
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

#[test]
fn backup_retention_policy_rejects_zero_or_inverted_windows() {
    let zero_start = BackupRetentionPolicy {
        pitr_window_start_lsn: 0,
        pitr_window_end_lsn: 200,
        expires_at_epoch: 1,
    };
    assert!(zero_start.validate().is_err());

    let zero_end = BackupRetentionPolicy {
        pitr_window_start_lsn: 100,
        pitr_window_end_lsn: 0,
        expires_at_epoch: 1,
    };
    assert!(zero_end.validate().is_err());

    let inverted = BackupRetentionPolicy {
        pitr_window_start_lsn: 200,
        pitr_window_end_lsn: 100,
        expires_at_epoch: 1,
    };
    assert!(inverted.validate().is_err());
}

#[test]
fn backup_retention_policy_predicate_is_safe_for_pitr_required_artifacts() {
    let policy = BackupRetentionPolicy {
        pitr_window_start_lsn: 100,
        pitr_window_end_lsn: 200,
        expires_at_epoch: 1_700_000_000,
    };

    assert!(policy.requires_pitr_artifact(100, 100).unwrap());
    assert!(policy.requires_pitr_artifact(80, 100).unwrap());
    assert!(policy.requires_pitr_artifact(150, 250).unwrap());
    assert!(!policy.requires_pitr_artifact(1, 99).unwrap());
    assert!(!policy.requires_pitr_artifact(201, 300).unwrap());

    let zero_start = policy.requires_pitr_artifact(0, 10).unwrap_err();
    assert!(zero_start.message().contains("must not be zero"));
    let inverted = policy.requires_pitr_artifact(10, 9).unwrap_err();
    assert!(inverted.message().contains("must not exceed"));
}

#[test]
fn backup_retention_policy_predicate_fails_closed_for_unsafe_policy_and_artifact_values() {
    let zero_expiry_policy = BackupRetentionPolicy {
        pitr_window_start_lsn: 100,
        pitr_window_end_lsn: 200,
        expires_at_epoch: 0,
    };
    let zero_expiry = zero_expiry_policy
        .requires_pitr_artifact(100, 150)
        .unwrap_err();
    assert!(
        zero_expiry
            .message()
            .contains("expiry epoch must not be zero")
    );

    let collapsed_window_policy = BackupRetentionPolicy {
        pitr_window_start_lsn: 100,
        pitr_window_end_lsn: 100,
        expires_at_epoch: 1,
    };
    let collapsed_window = collapsed_window_policy
        .requires_pitr_artifact(100, 150)
        .unwrap_err();
    assert!(collapsed_window.message().contains("must precede end LSN"));

    let policy = BackupRetentionPolicy {
        pitr_window_start_lsn: 100,
        pitr_window_end_lsn: 200,
        expires_at_epoch: 1,
    };
    let zero_artifact_end = policy.requires_pitr_artifact(100, 0).unwrap_err();
    assert!(zero_artifact_end.message().contains("must not be zero"));
}
