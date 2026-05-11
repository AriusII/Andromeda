use crate::support::*;
use andromeda_backup::{BackupId, FileBackedBackupArtifactStore};
use andromeda_restore::{
    RecoveryStage, RestoreCompletion, RestorePlanCompletionEvidenceV0, RestorePlanError,
    RestorePlanV0, RestoreValidationPolicy, WalSegmentToReplay, plan_replay_segments,
    validate_restore_artifact_preflight,
};
use andromeda_wal::Lsn;

#[test]
fn restore_drill_proof_succeeds_from_durable_preflight_to_completion_evidence() {
    let temp = tempfile::TempDir::new().unwrap();
    let backup_id = BackupId::new(91);
    write_test_artifact(&temp, backup_id);
    let target = Lsn::new(1500);
    let preflight = validate_restore_artifact_preflight(
        temp.path(),
        backup_id,
        target,
        RestoreValidationPolicy::Full,
    )
    .unwrap();

    let store = FileBackedBackupArtifactStore::open_existing(temp.path()).unwrap();
    let record = store.validate_artifact_directory(backup_id).unwrap();
    let manifest = record.manifest;
    let wal_segments = record.wal_segment_descriptors().unwrap();
    let audit = restore_audit_with_checksum(
        backup_id,
        target,
        RecoveryStage::SafeStart,
        preflight.restore_evidence_checksum(),
    );
    let orchestration = restore_orchestration_for(
        manifest.clone(),
        target,
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );

    orchestration.validate_with_preflight(&preflight).unwrap();
    let replay_plan = plan_replay_segments(&manifest, target, &wal_segments).unwrap();
    let plan = RestorePlanV0::from_orchestration(
        &orchestration,
        &replay_plan,
        preflight.restore_evidence_checksum(),
    )
    .unwrap();
    let evidence = RestorePlanCompletionEvidenceV0::bind(
        &plan,
        RestoreCompletion::Success {
            replayed_lsn: target,
            final_checkpoint_lsn: target,
        },
        preflight.restore_evidence_checksum(),
    )
    .unwrap();

    assert_eq!(evidence.backup_id, backup_id);
    assert_eq!(evidence.target_lsn, target);
    assert_eq!(evidence.replay_stop_lsn, plan.replay_stop_lsn());
}

#[test]
fn restore_drill_proof_fails_closed_when_manifest_artifact_missing() {
    let temp = tempfile::TempDir::new().unwrap();
    let backup_id = BackupId::new(92);
    let report = write_test_artifact(&temp, backup_id);
    std::fs::remove_file(report.manifest_path()).unwrap();

    let err = validate_restore_artifact_preflight(
        temp.path(),
        backup_id,
        Lsn::new(1500),
        RestoreValidationPolicy::Full,
    )
    .unwrap_err();

    assert!(
        err.message().contains("manifest"),
        "unexpected error: {err}"
    );
}

#[test]
fn restore_drill_proof_rejects_stale_preflight_checksum_on_completion_bind() {
    let temp = tempfile::TempDir::new().unwrap();
    let backup_id = BackupId::new(93);
    write_test_artifact(&temp, backup_id);
    let stale = validate_restore_artifact_preflight(
        temp.path(),
        backup_id,
        Lsn::new(1500),
        RestoreValidationPolicy::Full,
    )
    .unwrap();
    let current = validate_restore_artifact_preflight(
        temp.path(),
        backup_id,
        Lsn::new(1750),
        RestoreValidationPolicy::Full,
    )
    .unwrap();

    let store = FileBackedBackupArtifactStore::open_existing(temp.path()).unwrap();
    let record = store.validate_artifact_directory(backup_id).unwrap();
    let manifest = record.manifest;
    let wal_segments = record.wal_segment_descriptors().unwrap();
    let audit = restore_audit_with_checksum(
        backup_id,
        Lsn::new(1750),
        RecoveryStage::SafeStart,
        current.restore_evidence_checksum(),
    );
    let orchestration = restore_orchestration_for(
        manifest.clone(),
        Lsn::new(1750),
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );
    orchestration.validate_with_preflight(&current).unwrap();
    let replay_plan = plan_replay_segments(&manifest, Lsn::new(1750), &wal_segments).unwrap();
    let plan = RestorePlanV0::from_orchestration(
        &orchestration,
        &replay_plan,
        current.restore_evidence_checksum(),
    )
    .unwrap();

    let err = RestorePlanCompletionEvidenceV0::bind(
        &plan,
        RestoreCompletion::Success {
            replayed_lsn: Lsn::new(1750),
            final_checkpoint_lsn: Lsn::new(1750),
        },
        stale.restore_evidence_checksum(),
    )
    .unwrap_err();
    assert_eq!(err, RestorePlanError::PreflightChecksumMismatch);
}

#[test]
fn restore_drill_proof_rejects_replay_plan_without_target_coverage() {
    let temp = tempfile::TempDir::new().unwrap();
    let backup_id = BackupId::new(94);
    write_test_artifact(&temp, backup_id);
    let target = Lsn::new(1500);
    let preflight = validate_restore_artifact_preflight(
        temp.path(),
        backup_id,
        target,
        RestoreValidationPolicy::Full,
    )
    .unwrap();
    let store = FileBackedBackupArtifactStore::open_existing(temp.path()).unwrap();
    let record = store.validate_artifact_directory(backup_id).unwrap();
    let manifest = record.manifest;
    let wal = record.wal_segment_descriptors().unwrap();
    let replay_plan = vec![WalSegmentToReplay {
        segment_descriptor: wal[0],
        sequence_index: 0,
        contains_pitr_target: false,
        replay_stop_lsn: wal[0].last_lsn,
    }];
    let audit = restore_audit_with_checksum(
        backup_id,
        target,
        RecoveryStage::SafeStart,
        preflight.restore_evidence_checksum(),
    );
    let orchestration = restore_orchestration_for(
        manifest,
        target,
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );

    let err = RestorePlanV0::from_orchestration(
        &orchestration,
        &replay_plan,
        preflight.restore_evidence_checksum(),
    )
    .unwrap_err();
    assert_eq!(err, RestorePlanError::MissingReplayCoverage);
}

#[test]
fn restore_drill_proof_rejects_completion_lsn_mismatch_with_planned_stop() {
    let temp = tempfile::TempDir::new().unwrap();
    let backup_id = BackupId::new(95);
    write_test_artifact(&temp, backup_id);
    let target = Lsn::new(1500);
    let preflight = validate_restore_artifact_preflight(
        temp.path(),
        backup_id,
        target,
        RestoreValidationPolicy::Full,
    )
    .unwrap();
    let store = FileBackedBackupArtifactStore::open_existing(temp.path()).unwrap();
    let record = store.validate_artifact_directory(backup_id).unwrap();
    let manifest = record.manifest;
    let wal_segments = record.wal_segment_descriptors().unwrap();
    let replay_plan = plan_replay_segments(&manifest, target, &wal_segments).unwrap();
    let audit = restore_audit_with_checksum(
        backup_id,
        target,
        RecoveryStage::SafeStart,
        preflight.restore_evidence_checksum(),
    );
    let orchestration = restore_orchestration_for(
        manifest,
        target,
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );
    let plan = RestorePlanV0::from_orchestration(
        &orchestration,
        &replay_plan,
        preflight.restore_evidence_checksum(),
    )
    .unwrap();

    let err = RestorePlanCompletionEvidenceV0::bind(
        &plan,
        RestoreCompletion::Success {
            replayed_lsn: Lsn::new(target.get() - 1),
            final_checkpoint_lsn: Lsn::new(target.get() - 1),
        },
        preflight.restore_evidence_checksum(),
    )
    .unwrap_err();
    assert_eq!(err, RestorePlanError::CompletionReplayMismatch);
}
