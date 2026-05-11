use crate::support::test_manifest;
use crate::support::{
    BackupExecutionPlan, TEST_AUDIT_LEDGER_BYTES, TEST_CATALOG_BYTES, backup_plan,
    cold_extent_copy_task, temp_dir, test_extent, test_wal_segment,
};
use andromeda_backup::{FileBackedBackupArtifactStore, RecoverableGate, RestoreEvidence};
use andromeda_segment::ExtentState;

fn test_execution_plan_for_publication(backup_id: u64) -> BackupExecutionPlan {
    let snapshot_bytes = b"snapshot-publication-bytes";
    let wal_bytes = b"wal-publication-bytes";
    let manifest = test_manifest(backup_id, 101, 200);
    let extent = test_extent(1, 1000, 1, ExtentState::PublishedCold);
    let wal_segment = test_wal_segment(10, 101, 200, None);
    backup_plan(
        manifest,
        vec![cold_extent_copy_task(extent, snapshot_bytes.len() as u64)],
        vec![crate::support::wal_segment_copy_task(
            wal_segment,
            wal_bytes.len() as u64,
            0,
        )],
        true,
        snapshot_bytes.len() as u64,
        wal_bytes.len() as u64,
    )
}

fn write_report_for_publication(backup_id: u64) -> andromeda_backup::BackupArtifactWriteReport {
    let plan = test_execution_plan_for_publication(backup_id);
    let temp = temp_dir();
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();
    store
        .write_execution_plan_artifact(
            &plan,
            b"snapshot-publication-bytes",
            &[b"wal-publication-bytes".as_slice()],
            TEST_CATALOG_BYTES,
            TEST_AUDIT_LEDGER_BYTES,
        )
        .unwrap()
}

#[test]
fn backup_is_not_recoverable_without_restore_evidence() {
    let gate = RecoverableGate::new(test_manifest(80, 101, 200));
    assert!(!gate.is_recoverable());
    assert!(gate.restore_evidence().is_none());
}

#[test]
fn backup_becomes_recoverable_only_after_valid_restore_evidence() {
    let manifest = test_manifest(81, 101, 200);
    let evidence = RestoreEvidence::new(
        manifest.backup_id,
        manifest.manifest_crc,
        1,
        1_700_000_000,
        200,
        [9; 32],
    )
    .unwrap();
    let gate = RecoverableGate::new(manifest);

    let finalized = gate.attach_restore_evidence(evidence).unwrap();
    assert!(finalized.is_recoverable());
    assert_eq!(
        finalized.restore_evidence().unwrap().target_lsn_verified(),
        200
    );
}

#[test]
fn recoverable_gate_rejects_second_evidence_attachment() {
    let manifest = test_manifest(82, 101, 200);
    let first = RestoreEvidence::new(
        manifest.backup_id,
        manifest.manifest_crc,
        1,
        1_700_000_000,
        200,
        [1; 32],
    )
    .unwrap();
    let second = RestoreEvidence::new(
        manifest.backup_id,
        manifest.manifest_crc,
        2,
        1_700_000_001,
        200,
        [2; 32],
    )
    .unwrap();
    let gate = RecoverableGate::new(manifest);

    let finalized = gate.attach_restore_evidence(first).unwrap();
    assert!(finalized.attach_restore_evidence(second).is_err());
}

#[test]
fn restore_evidence_constructor_fails_closed_for_zero_or_empty_fields() {
    let manifest = test_manifest(83, 101, 200);
    let zero_id = RestoreEvidence::new(
        manifest.backup_id,
        manifest.manifest_crc,
        0,
        1_700_000_000,
        100,
        [1; 32],
    )
    .unwrap_err();
    assert!(zero_id.message().contains("must not be zero"));

    let zero_digest = RestoreEvidence::new(
        manifest.backup_id,
        manifest.manifest_crc,
        1,
        1_700_000_000,
        100,
        [0; 32],
    )
    .unwrap_err();
    assert!(zero_digest.message().contains("must not be all-zero"));

    let zero_target = RestoreEvidence::new(
        manifest.backup_id,
        manifest.manifest_crc,
        1,
        1_700_000_000,
        0,
        [3; 32],
    )
    .unwrap_err();
    assert!(
        zero_target
            .message()
            .contains("target LSN verified must not be zero")
    );
}

#[test]
fn recoverable_gate_rejects_restore_evidence_for_different_manifest() {
    let manifest = test_manifest(84, 101, 200);
    let other_manifest = test_manifest(85, 101, 200);
    let evidence = RestoreEvidence::new(
        other_manifest.backup_id,
        other_manifest.manifest_crc,
        1,
        1_700_000_000,
        200,
        [8; 32],
    )
    .unwrap();

    let err = RecoverableGate::new(manifest)
        .attach_restore_evidence(evidence)
        .expect_err("restore evidence for another backup must be rejected");
    assert!(
        err.to_string().contains("backup id must match"),
        "unexpected error: {err}"
    );
}

#[test]
fn recoverable_gate_rejects_restore_evidence_for_manifest_crc_mismatch() {
    let manifest = test_manifest(85, 101, 200);
    let evidence = RestoreEvidence::new(
        manifest.backup_id,
        manifest.manifest_crc.wrapping_add(1),
        1,
        1_700_000_000,
        200,
        [6; 32],
    )
    .unwrap();

    let err = RecoverableGate::new(manifest)
        .attach_restore_evidence(evidence)
        .expect_err("restore evidence with a mismatched manifest CRC must be rejected");
    assert!(
        err.to_string().contains("manifest CRC must match"),
        "unexpected error: {err}"
    );
}

#[test]
fn recoverable_gate_rejects_restore_evidence_outside_manifest_pitr_range() {
    let manifest = test_manifest(86, 101, 200);
    let evidence = RestoreEvidence::new(
        manifest.backup_id,
        manifest.manifest_crc,
        1,
        1_700_000_000,
        201,
        [7; 32],
    )
    .unwrap();

    let err = RecoverableGate::new(manifest)
        .attach_restore_evidence(evidence)
        .expect_err("restore evidence outside PITR range must be rejected");
    assert!(
        err.to_string().contains("PITR range"),
        "unexpected error: {err}"
    );
}

#[test]
fn recoverable_gate_rejects_restore_evidence_before_snapshot_base_checkpoint() {
    let manifest = test_manifest(87, 101, 200);
    let evidence = RestoreEvidence::new(
        manifest.backup_id,
        manifest.manifest_crc,
        1,
        1_700_000_000,
        99,
        [5; 32],
    )
    .unwrap();

    let err = RecoverableGate::new(manifest)
        .attach_restore_evidence(evidence)
        .expect_err("restore evidence before snapshot base checkpoint must be rejected");
    assert!(
        err.to_string().contains("PITR range"),
        "unexpected error: {err}"
    );
}

#[test]
fn publication_report_gate_is_fail_closed_without_restore_evidence() {
    let report = write_report_for_publication(90);
    let gate = report.recoverable_gate();
    assert!(!gate.is_recoverable());
    assert!(gate.restore_evidence().is_none());
}

#[test]
fn publication_recoverable_finalize_rejects_mismatched_evidence() {
    let report = write_report_for_publication(91);
    let evidence = RestoreEvidence::new(
        andromeda_backup::BackupId::new(999),
        report.manifest().manifest_crc,
        1,
        1_700_000_000,
        200,
        [1; 32],
    )
    .unwrap();

    let err = report
        .finalize_recoverable_publication(evidence)
        .expect_err("mismatched evidence must fail recoverable finalization");
    assert!(
        err.to_string().contains("backup id must match"),
        "unexpected error: {err}"
    );
}

#[test]
fn publication_recoverable_finalize_succeeds_with_manifest_bound_evidence() {
    let report = write_report_for_publication(92);
    let evidence = RestoreEvidence::new(
        report.backup_id(),
        report.manifest().manifest_crc,
        7,
        1_700_000_001,
        200,
        [9; 32],
    )
    .unwrap();

    let recoverable = report
        .finalize_recoverable_publication(evidence)
        .expect("valid evidence should finalize recoverable publication");
    assert_eq!(
        recoverable.write_report().backup_id(),
        recoverable.restore_evidence().backup_id()
    );
}

#[test]
fn publication_gate_rejects_double_attachment() {
    let report = write_report_for_publication(93);
    let first = RestoreEvidence::new(
        report.backup_id(),
        report.manifest().manifest_crc,
        1,
        1_700_000_000,
        200,
        [3; 32],
    )
    .unwrap();
    let second = RestoreEvidence::new(
        report.backup_id(),
        report.manifest().manifest_crc,
        2,
        1_700_000_001,
        200,
        [4; 32],
    )
    .unwrap();

    let finalized = report
        .recoverable_gate()
        .attach_restore_evidence(first)
        .unwrap();
    assert!(finalized.attach_restore_evidence(second).is_err());
}
