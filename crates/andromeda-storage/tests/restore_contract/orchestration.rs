use crate::support::*;
use andromeda_backup::BackupId;
use andromeda_restore::{RecoveryStage, RestoreValidationPolicy};
use andromeda_wal::Lsn;

#[test]
fn test_restore_orchestration_constructs_successfully() {
    let manifest = make_test_manifest();
    let audit = restore_audit_for(&manifest, Lsn::new(1500), RecoveryStage::SafeStart);

    let orch = restore_orchestration_for(
        manifest,
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );

    assert_eq!(orch.pitr_target_lsn, Lsn::new(1500));
    assert_eq!(orch.recovery_stage, RecoveryStage::SafeStart);
}

#[test]
fn test_restore_orchestration_validates_manifest() {
    let manifest = make_test_manifest();
    let audit = restore_audit_for(&manifest, Lsn::new(1500), RecoveryStage::SafeStart);

    let orch = restore_orchestration_for(
        manifest,
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );

    assert!(orch.validate().is_ok());
}

#[test]
fn test_restore_orchestration_rejects_pitr_out_of_range() {
    let manifest = make_test_manifest();
    let audit = restore_audit_for(&manifest, Lsn::new(3000), RecoveryStage::SafeStart);

    let orch = restore_orchestration_for(
        manifest,
        Lsn::new(3000),
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );

    assert!(orch.validate().is_err());
}

#[test]
fn test_restore_orchestration_rejects_audit_target_mismatch() {
    let manifest = make_test_manifest();
    let audit = restore_audit_for(&manifest, Lsn::new(1501), RecoveryStage::SafeStart);

    let orch = restore_orchestration_for(
        manifest,
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );

    let err = orch
        .validate()
        .expect_err("restore audit must bind the orchestration PITR target");
    assert!(
        err.message().contains("PITR target"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_restore_orchestration_rejects_audit_stage_mismatch() {
    let manifest = make_test_manifest();
    let audit = restore_audit_for(&manifest, Lsn::new(1500), RecoveryStage::ForensicStart);

    let orch = restore_orchestration_for(
        manifest,
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );

    let err = orch
        .validate()
        .expect_err("restore audit must bind the recovery stage");
    assert!(
        err.message().contains("recovery stage"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_restore_orchestration_rejects_audit_checksum_mismatch() {
    let manifest = make_test_manifest();
    let audit = restore_audit_with_checksum(
        BackupId::new(1),
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        andromeda_restore::compute_restore_checksum(&manifest).wrapping_add(1),
    );

    let orch = restore_orchestration_for(
        manifest,
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Full,
        audit,
    );

    let err = orch
        .validate()
        .expect_err("restore audit checksum must bind the manifest");
    assert!(
        err.message().contains("checksum"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_restore_orchestration_supports_forensic_start() {
    let manifest = make_test_manifest();
    let audit = restore_audit_for(&manifest, Lsn::new(1500), RecoveryStage::ForensicStart);

    let orch = restore_orchestration_for(
        manifest,
        Lsn::new(1500),
        RecoveryStage::ForensicStart,
        RestoreValidationPolicy::Full,
        audit,
    );

    assert!(orch.validate().is_ok());
    assert_eq!(orch.recovery_stage, RecoveryStage::ForensicStart);
    assert!(!orch.recovery_stage.application_traffic_allowed());
    assert!(!orch.recovery_stage.durable_truth_mutation_allowed());
    assert!(!orch.recovery_stage.in_place_repair_allowed());
}

#[test]
fn test_restore_orchestration_supports_minimal_validation() {
    let manifest = make_test_manifest();
    let audit = restore_audit_for(&manifest, Lsn::new(1500), RecoveryStage::SafeStart);

    let orch = restore_orchestration_for(
        manifest,
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Minimal,
        audit,
    );

    assert!(orch.validate().is_ok());
    assert_eq!(orch.validation_policy, RestoreValidationPolicy::Minimal);
}

#[test]
fn test_forensic_start_rejects_minimal_validation_policy() {
    let manifest = make_test_manifest();
    let audit = restore_audit_for(&manifest, Lsn::new(1500), RecoveryStage::ForensicStart);

    let orch = restore_orchestration_for(
        manifest,
        Lsn::new(1500),
        RecoveryStage::ForensicStart,
        RestoreValidationPolicy::Minimal,
        audit,
    );

    let err = orch
        .validate()
        .expect_err("ForensicStart must not use minimal restore validation");
    assert!(
        err.message().contains("full validation"),
        "unexpected error: {err}"
    );
}
