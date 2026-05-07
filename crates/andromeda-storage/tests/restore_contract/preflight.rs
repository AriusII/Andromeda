use crate::support::*;
use andromeda_storage::{
    BackupId, Lsn, RecoveryStage, RestoreValidationPolicy, validate_restore_artifact_preflight,
};

#[test]
fn restore_preflight_accepts_file_backed_artifact_directory() {
    let temp = tempfile::TempDir::new().unwrap();
    let backup_id = BackupId::new(77);
    let report = write_test_artifact(&temp, backup_id);

    let preflight = validate_restore_artifact_preflight(
        temp.path(),
        backup_id,
        Lsn::new(1500),
        RestoreValidationPolicy::Full,
    )
    .unwrap();

    assert_eq!(preflight.backup_id, backup_id);
    assert_eq!(
        preflight.manifest_format_version,
        CURRENT_ARTIFACT_MANIFEST_FORMAT_VERSION
    );
    assert_eq!(preflight.source_checkpoint_lsn, Lsn::new(1000));
    assert_eq!(preflight.replay_segment_count, 1);
    assert_ne!(preflight.restore_evidence_checksum, 0);
    assert_eq!(
        preflight.manifest_digest,
        report.artifact_set.backup_manifest
    );
    assert_eq!(preflight.wal_archive_evidence.end_lsn, Lsn::new(2000));
}

#[test]
fn restore_preflight_accepts_legacy_v1_manifest_after_reconstructing_wal_evidence() {
    let temp = tempfile::TempDir::new().unwrap();
    let backup_id = BackupId::new(80);
    let report = write_test_artifact(&temp, backup_id);
    rewrite_manifest_to_v1_without_archive_digest(&report.manifest_path);

    let preflight = validate_restore_artifact_preflight(
        temp.path(),
        backup_id,
        Lsn::new(1500),
        RestoreValidationPolicy::Full,
    )
    .unwrap();

    assert_eq!(
        preflight.manifest_format_version,
        LEGACY_V1_ARTIFACT_MANIFEST_FORMAT_VERSION
    );
    assert_ne!(preflight.restore_evidence_checksum, 0);
    assert_eq!(
        preflight.wal_archive_evidence.archive_digest_sha256,
        report.wal_archive_evidence.archive_digest_sha256
    );
}

#[test]
fn restore_orchestration_forensic_start_binds_preflight_evidence_checksum() {
    let temp = tempfile::TempDir::new().unwrap();
    let backup_id = BackupId::new(82);
    let preflight = {
        write_test_artifact(&temp, backup_id);
        validate_restore_artifact_preflight(
            temp.path(),
            backup_id,
            Lsn::new(1500),
            RestoreValidationPolicy::Full,
        )
        .unwrap()
    };
    let mut manifest = make_test_manifest();
    manifest.backup_id = backup_id;
    let audit = restore_audit_with_checksum(
        backup_id,
        Lsn::new(1500),
        RecoveryStage::ForensicStart,
        preflight.restore_evidence_checksum,
    );

    let orchestration = restore_orchestration_for(
        manifest,
        Lsn::new(1500),
        RecoveryStage::ForensicStart,
        RestoreValidationPolicy::Full,
        audit,
    );

    orchestration
        .validate_with_preflight(&preflight)
        .expect("forensic restore must bind durable preflight evidence");
}

#[test]
fn restore_orchestration_rejects_preflight_evidence_checksum_mismatch() {
    let temp = tempfile::TempDir::new().unwrap();
    let backup_id = BackupId::new(83);
    let preflight = {
        write_test_artifact(&temp, backup_id);
        validate_restore_artifact_preflight(
            temp.path(),
            backup_id,
            Lsn::new(1500),
            RestoreValidationPolicy::Full,
        )
        .unwrap()
    };
    let mut manifest = make_test_manifest();
    manifest.backup_id = backup_id;
    let audit = restore_audit_with_checksum(
        backup_id,
        Lsn::new(1500),
        RecoveryStage::ForensicStart,
        preflight.restore_evidence_checksum.wrapping_add(1),
    );

    let orchestration = restore_orchestration_for(
        manifest,
        Lsn::new(1500),
        RecoveryStage::ForensicStart,
        RestoreValidationPolicy::Full,
        audit,
    );

    let err = orchestration
        .validate_with_preflight(&preflight)
        .expect_err("restore audit must bind preflight evidence checksum");
    assert!(
        err.message().contains("preflight evidence checksum"),
        "unexpected error: {err}"
    );
}

#[test]
fn restore_preflight_rejects_missing_wal_file() {
    let temp = tempfile::TempDir::new().unwrap();
    let backup_id = BackupId::new(78);
    let report = write_test_artifact(&temp, backup_id);
    std::fs::remove_file(&report.wal_segment_paths[0]).unwrap();

    let err = validate_restore_artifact_preflight(
        temp.path(),
        backup_id,
        Lsn::new(1500),
        RestoreValidationPolicy::Full,
    )
    .unwrap_err();

    assert!(
        err.message().contains("backup WAL segment"),
        "unexpected error: {err}"
    );
}

#[test]
fn restore_preflight_rejects_corrupted_manifest_payload() {
    let temp = tempfile::TempDir::new().unwrap();
    let backup_id = BackupId::new(79);
    let report = write_test_artifact(&temp, backup_id);
    let mut bytes = std::fs::read(&report.manifest_path).unwrap();
    let last = bytes.last_mut().unwrap();
    *last ^= 0x01;
    std::fs::write(&report.manifest_path, bytes).unwrap();

    let err = validate_restore_artifact_preflight(
        temp.path(),
        backup_id,
        Lsn::new(1500),
        RestoreValidationPolicy::Full,
    )
    .unwrap_err();

    assert!(
        err.message().contains("manifest payload checksum mismatch"),
        "unexpected error: {err}"
    );
}

#[test]
fn restore_preflight_rejects_corrupted_wal_archive_evidence_digest() {
    let temp = tempfile::TempDir::new().unwrap();
    let backup_id = BackupId::new(81);
    let report = write_test_artifact(&temp, backup_id);
    zero_manifest_archive_digest(&report.manifest_path);

    let err = validate_restore_artifact_preflight(
        temp.path(),
        backup_id,
        Lsn::new(1500),
        RestoreValidationPolicy::Full,
    )
    .unwrap_err();

    assert!(
        err.message().contains("evidence digest"),
        "unexpected error: {err}"
    );
}
