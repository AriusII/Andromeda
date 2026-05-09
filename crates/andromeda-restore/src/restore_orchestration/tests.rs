use super::*;
use andromeda_backup::{BackupId, BackupManifest, ColdSnapshotBoundary, WalArchiveRange};
use andromeda_wal::Lsn;

fn make_test_manifest() -> BackupManifest<Lsn> {
    BackupManifest {
        backup_id: BackupId::new(1),
        database_id: 1,
        created_epoch: 1,
        snapshot: ColdSnapshotBoundary {
            snapshot_id: 100,
            snapshot_descriptor_hash: [0xAB; 32],
            base_checkpoint_lsn: Lsn::new(1000),
            required_wal_start_lsn: Lsn::new(1001),
        },
        wal_archive: WalArchiveRange::new(Lsn::new(1001), Lsn::new(2000)),
        manifest_crc: 1234,
    }
}

#[test]
fn validate_restore_prerequisites_accepts_valid_manifest() {
    let manifest = make_test_manifest();
    let pitr_lsn = Lsn::new(1500);

    assert!(validate_restore_prerequisites(&manifest, pitr_lsn).is_ok());
}

#[test]
fn validate_restore_prerequisites_rejects_pitr_before_required_wal_start() {
    let mut manifest = make_test_manifest();
    manifest.snapshot.required_wal_start_lsn = Lsn::new(1005);
    manifest.wal_archive = WalArchiveRange::new(Lsn::new(900), Lsn::new(2000));

    let err = validate_restore_prerequisites(&manifest, Lsn::new(1002))
        .expect_err("PITR before required WAL start must fail closed");

    assert!(
        err.message().contains("required WAL start"),
        "unexpected error: {err}"
    );
}

#[test]
fn validate_restore_prerequisites_rejects_pitr_below_range() {
    let manifest = make_test_manifest();
    let pitr_lsn = Lsn::new(999);

    assert!(validate_restore_prerequisites(&manifest, pitr_lsn).is_err());
}

#[test]
fn validate_restore_prerequisites_rejects_pitr_above_range() {
    let manifest = make_test_manifest();
    let pitr_lsn = Lsn::new(2001);

    assert!(validate_restore_prerequisites(&manifest, pitr_lsn).is_err());
}

#[test]
fn compute_restore_checksum_is_deterministic() {
    let manifest = make_test_manifest();
    let checksum1 = compute_restore_checksum(&manifest);
    let checksum2 = compute_restore_checksum(&manifest);

    assert_eq!(checksum1, checksum2);
}

#[test]
fn plan_replay_segments_requires_segments() {
    let manifest = make_test_manifest();
    let pitr_lsn = Lsn::new(1500);

    assert!(plan_replay_segments(&manifest, pitr_lsn, &[]).is_err());
}

#[test]
fn plan_replay_segments_skips_wal_for_snapshot_base_target() {
    let manifest = make_test_manifest();
    let pitr_lsn = Lsn::new(1000);

    let plan = plan_replay_segments(&manifest, pitr_lsn, &[])
        .expect("snapshot base checkpoint target must not need WAL");
    assert!(plan.is_empty());
}

#[test]
fn restore_audit_trace_rejects_zero_trace_id() {
    let trace = RestoreAuditTrace::new(
        andromeda_observe::TraceId::new(0),
        BackupId::new(1),
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        12345,
    );

    assert!(trace.validate().is_err());
}

#[test]
fn restore_audit_trace_rejects_zero_backup_id() {
    let trace = RestoreAuditTrace::new(
        andromeda_observe::TraceId::new(1),
        BackupId::new(0),
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        12345,
    );

    assert!(trace.validate().is_err());
}

#[test]
fn restore_orchestration_validates_prerequisites() {
    let manifest = make_test_manifest();
    let audit = RestoreAuditTrace::new(
        andromeda_observe::TraceId::new(1),
        BackupId::new(1),
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        compute_restore_checksum(&manifest),
    );

    let orch = RestoreOrchestration::new(
        manifest,
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );

    assert!(orch.validate().is_ok());
}

#[test]
fn restore_orchestration_rejects_audit_binding_mismatch() {
    let manifest = make_test_manifest();
    let audit = RestoreAuditTrace::new(
        andromeda_observe::TraceId::new(1),
        BackupId::new(1),
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        compute_restore_checksum(&manifest).wrapping_add(1),
    );

    let orch = RestoreOrchestration::new(
        manifest,
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );

    assert!(orch.validate().is_err());
}

#[test]
fn restore_orchestration_rejects_invalid_pitr() {
    let manifest = make_test_manifest();
    let audit = RestoreAuditTrace::new(
        andromeda_observe::TraceId::new(1),
        BackupId::new(1),
        Lsn::new(3000),
        RecoveryStage::SafeStart,
        compute_restore_checksum(&manifest),
    );

    let orch = RestoreOrchestration::new(
        manifest,
        Lsn::new(3000),
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );

    assert!(orch.validate().is_err());
}
