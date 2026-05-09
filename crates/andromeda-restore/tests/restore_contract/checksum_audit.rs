use crate::support::*;
use andromeda_backup::{BackupId, WalArchiveRange};
use andromeda_observe::TraceId;
use andromeda_restore::{
    RecoveryStage, RestoreAuditTrace, RestoreCompletion, compute_restore_checksum,
};
use andromeda_wal::Lsn;

#[test]
fn test_restore_checksum_deterministic() {
    let manifest = make_test_manifest();
    let checksum1 = compute_restore_checksum(&manifest);
    let checksum2 = compute_restore_checksum(&manifest);

    assert_eq!(checksum1, checksum2);
}

#[test]
fn test_restore_checksum_varies_with_manifest() {
    let manifest1 = make_test_manifest();
    let mut manifest2 = make_test_manifest();

    manifest2.backup_id = BackupId::new(2);

    let checksum1 = compute_restore_checksum(&manifest1);
    let checksum2 = compute_restore_checksum(&manifest2);

    assert_ne!(checksum1, checksum2);
}

#[test]
fn test_restore_checksum_varies_with_wal_range() {
    let manifest1 = make_test_manifest();
    let mut manifest2 = make_test_manifest();

    manifest2.wal_archive = WalArchiveRange::new(Lsn::new(1001), Lsn::new(3000));

    let checksum1 = compute_restore_checksum(&manifest1);
    let checksum2 = compute_restore_checksum(&manifest2);

    assert_ne!(checksum1, checksum2);
}

#[test]
fn test_restore_checksum_varies_with_required_wal_start() {
    let manifest1 = make_test_manifest();
    let mut manifest2 = make_test_manifest();
    manifest2.snapshot.required_wal_start_lsn = Lsn::new(1002);
    manifest2.wal_archive = WalArchiveRange::new(Lsn::new(1002), Lsn::new(2000));

    let checksum1 = compute_restore_checksum(&manifest1);
    let checksum2 = compute_restore_checksum(&manifest2);

    assert_ne!(checksum1, checksum2);
}

#[test]
fn test_restore_checksum_varies_with_snapshot_descriptor_hash() {
    let manifest1 = make_test_manifest();
    let mut manifest2 = make_test_manifest();
    manifest2.snapshot.snapshot_descriptor_hash = [0xCD; 32];

    let checksum1 = compute_restore_checksum(&manifest1);
    let checksum2 = compute_restore_checksum(&manifest2);

    assert_ne!(checksum1, checksum2);
}

#[test]
fn test_audit_trace_accepts_valid_inputs() {
    let trace = RestoreAuditTrace::new(
        TraceId::new(1),
        BackupId::new(1),
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        12345,
    );

    assert!(trace.validate().is_ok());
}

#[test]
fn test_audit_trace_rejects_zero_trace_id() {
    let trace = RestoreAuditTrace::new(
        TraceId::new(0),
        BackupId::new(1),
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        12345,
    );

    assert!(trace.validate().is_err());
}

#[test]
fn test_audit_trace_rejects_zero_backup_id() {
    let trace = RestoreAuditTrace::new(
        TraceId::new(1),
        BackupId::new(0),
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        12345,
    );

    assert!(trace.validate().is_err());
}

#[test]
fn test_audit_trace_binds_completion_success() {
    let trace = RestoreAuditTrace::new(
        TraceId::new(1),
        BackupId::new(1),
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        12345,
    );

    let completed = trace.with_completion(RestoreCompletion::Success {
        replayed_lsn: Lsn::new(1500),
        final_checkpoint_lsn: Lsn::new(1500),
    });

    assert!(completed.completion_status.is_some());
    match completed.completion_status {
        Some(RestoreCompletion::Success { .. }) => {},
        _ => panic!("Expected success"),
    }
}

#[test]
fn test_audit_trace_binds_completion_failed() {
    let trace = RestoreAuditTrace::new(
        TraceId::new(1),
        BackupId::new(1),
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        12345,
    );

    let completed = trace.with_completion(RestoreCompletion::Failed {
        reason: "WAL gap detected".to_string(),
    });

    assert!(completed.completion_status.is_some());
    match completed.completion_status {
        Some(RestoreCompletion::Failed { reason }) => {
            assert_eq!(reason, "WAL gap detected");
        },
        _ => panic!("Expected failure"),
    }
}
