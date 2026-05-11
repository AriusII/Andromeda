use crate::support::*;
use andromeda_backup::BackupId;
use andromeda_restore::{
    RecoveryStage, RestoreValidationPolicy, validate_restore_artifact_preflight,
};
use andromeda_wal::Lsn;

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

    assert_eq!(preflight.backup_id(), backup_id);
    assert_eq!(
        preflight.manifest_format_version(),
        CURRENT_ARTIFACT_MANIFEST_FORMAT_VERSION
    );
    assert_eq!(preflight.source_checkpoint_lsn(), Lsn::new(1000));
    assert_eq!(preflight.replay_segment_count(), 1);
    assert_ne!(preflight.restore_evidence_checksum(), 0);
    assert_eq!(
        preflight.manifest_digest(),
        report.artifact_set().backup_manifest
    );
    assert_eq!(
        preflight.catalog_digest(),
        report.artifact_set().catalog.artifact
    );
    assert_eq!(
        preflight.audit_ledger_digest(),
        report.artifact_set().audit_ledger.artifact
    );
    assert_eq!(preflight.wal_archive_evidence().end_lsn, Lsn::new(2000));
}

#[test]
fn restore_preflight_evidence_checksum_binds_pitr_target() {
    let temp = tempfile::TempDir::new().unwrap();
    let backup_id = BackupId::new(84);
    write_test_artifact(&temp, backup_id);

    let first_target = validate_restore_artifact_preflight(
        temp.path(),
        backup_id,
        Lsn::new(1500),
        RestoreValidationPolicy::Full,
    )
    .unwrap();
    let second_target = validate_restore_artifact_preflight(
        temp.path(),
        backup_id,
        Lsn::new(1750),
        RestoreValidationPolicy::Full,
    )
    .unwrap();

    assert_eq!(
        first_target.manifest_digest(),
        second_target.manifest_digest()
    );
    assert_eq!(
        first_target.snapshot_digest(),
        second_target.snapshot_digest()
    );
    assert_eq!(
        first_target.catalog_digest(),
        second_target.catalog_digest()
    );
    assert_eq!(
        first_target.audit_ledger_digest(),
        second_target.audit_ledger_digest()
    );
    assert_eq!(
        first_target.wal_archive_evidence(),
        second_target.wal_archive_evidence()
    );
    assert_ne!(
        first_target.restore_evidence_checksum(),
        second_target.restore_evidence_checksum(),
        "restore evidence must bind the operator-selected PITR target"
    );
}

#[test]
fn restore_preflight_evidence_checksum_binds_catalog_and_audit_artifacts() {
    let first_temp = tempfile::TempDir::new().unwrap();
    let second_temp = tempfile::TempDir::new().unwrap();
    let backup_id = BackupId::new(89);
    write_test_artifact_with_catalog_audit(
        &first_temp,
        backup_id,
        b"catalog artifact one",
        b"audit ledger artifact one",
    );
    write_test_artifact_with_catalog_audit(
        &second_temp,
        backup_id,
        b"catalog artifact two",
        b"audit ledger artifact two",
    );

    let first = validate_restore_artifact_preflight(
        first_temp.path(),
        backup_id,
        Lsn::new(1500),
        RestoreValidationPolicy::Full,
    )
    .unwrap();
    let second = validate_restore_artifact_preflight(
        second_temp.path(),
        backup_id,
        Lsn::new(1500),
        RestoreValidationPolicy::Full,
    )
    .unwrap();

    assert_ne!(
        first.manifest_digest(),
        second.manifest_digest(),
        "v4 manifest digest must also bind catalog and audit-ledger metadata"
    );
    assert_eq!(first.snapshot_digest(), second.snapshot_digest());
    assert_ne!(first.catalog_digest(), second.catalog_digest());
    assert_ne!(first.audit_ledger_digest(), second.audit_ledger_digest());
    assert_ne!(
        first.restore_evidence_checksum(),
        second.restore_evidence_checksum(),
        "restore preflight evidence must bind catalog and audit-ledger artifacts"
    );
}

#[test]
fn restore_orchestration_rejects_preflight_source_checkpoint_mismatch() {
    let temp = tempfile::TempDir::new().unwrap();
    let backup_id = BackupId::new(85);
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
    manifest.snapshot.base_checkpoint_lsn = Lsn::new(999);
    let audit = restore_audit_with_checksum(
        backup_id,
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        preflight.restore_evidence_checksum(),
    );

    let orchestration = restore_orchestration_for(
        manifest,
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );

    let err = orchestration
        .validate_with_preflight(&preflight)
        .expect_err("restore orchestration must bind the preflight manifest");
    assert!(
        err.message().contains("backup manifest"),
        "unexpected error: {err}"
    );
}

#[test]
fn restore_preflight_rejects_legacy_v1_manifest_without_catalog_audit_bindings() {
    let temp = tempfile::TempDir::new().unwrap();
    let backup_id = BackupId::new(80);
    let report = write_test_artifact(&temp, backup_id);
    rewrite_manifest_to_v1_without_archive_digest(report.manifest_path());

    let err = validate_restore_artifact_preflight(
        temp.path(),
        backup_id,
        Lsn::new(1500),
        RestoreValidationPolicy::Full,
    )
    .unwrap_err();

    assert!(
        err.message().contains("catalog and audit-ledger bindings"),
        "unexpected error: {err}"
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
        preflight.restore_evidence_checksum(),
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
        preflight.restore_evidence_checksum().wrapping_add(1),
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
    std::fs::remove_file(&report.wal_segment_paths()[0]).unwrap();

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
fn restore_preflight_rejects_missing_catalog_file() {
    let temp = tempfile::TempDir::new().unwrap();
    let backup_id = BackupId::new(88);
    let report = write_test_artifact(&temp, backup_id);
    std::fs::remove_file(report.catalog_path()).unwrap();

    let err = validate_restore_artifact_preflight(
        temp.path(),
        backup_id,
        Lsn::new(1500),
        RestoreValidationPolicy::Full,
    )
    .unwrap_err();

    assert!(
        err.message().contains("backup catalog"),
        "unexpected error: {err}"
    );
}

#[test]
fn restore_preflight_rejects_corrupted_manifest_payload() {
    let temp = tempfile::TempDir::new().unwrap();
    let backup_id = BackupId::new(79);
    let report = write_test_artifact(&temp, backup_id);
    let mut bytes = std::fs::read(report.manifest_path()).unwrap();
    let last = bytes.last_mut().unwrap();
    *last ^= 0x01;
    std::fs::write(report.manifest_path(), bytes).unwrap();

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
    zero_manifest_archive_digest(report.manifest_path());

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
