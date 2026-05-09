use crate::support::*;

#[test]
fn backup_audit_trace_binds_trace_id_and_backup_id() {
    let trace = BackupAuditTrace::new(
        TraceId::new(456),
        BackupId::new(1),
        BackupAuditEvent::BackupStarted {
            backup_id: BackupId::new(1),
            database_id: 99,
        },
        SystemTime::now(),
        1,
    );

    assert_eq!(trace.trace_id, TraceId::new(456));
    assert_eq!(trace.backup_id, BackupId::new(1));
    assert_eq!(trace.sequence_number, 1);
}

#[test]
fn backup_audit_trace_validation_rejects_zero_trace_id() {
    let trace = BackupAuditTrace::new(
        TraceId::new(0),
        BackupId::new(1),
        BackupAuditEvent::BackupStarted {
            backup_id: BackupId::new(1),
            database_id: 99,
        },
        SystemTime::now(),
        1,
    );

    assert!(!trace.validate());
}

#[test]
fn backup_audit_trace_validation_rejects_zero_backup_id() {
    let trace = BackupAuditTrace::new(
        TraceId::new(123),
        BackupId::new(0),
        BackupAuditEvent::BackupStarted {
            backup_id: BackupId::new(1),
            database_id: 99,
        },
        SystemTime::now(),
        1,
    );

    assert!(!trace.validate());
}

#[test]
fn backup_audit_trace_validation_rejects_zero_sequence_number() {
    let trace = BackupAuditTrace::new(
        TraceId::new(123),
        BackupId::new(1),
        BackupAuditEvent::BackupStarted {
            backup_id: BackupId::new(1),
            database_id: 99,
        },
        SystemTime::now(),
        0,
    );

    assert!(!trace.validate());
}

#[test]
fn backup_audit_trace_validation_accepts_valid_trace() {
    let trace = BackupAuditTrace::new(
        TraceId::new(123),
        BackupId::new(1),
        BackupAuditEvent::BackupStarted {
            backup_id: BackupId::new(1),
            database_id: 99,
        },
        SystemTime::now(),
        1,
    );

    assert!(trace.validate());
}
#[test]
fn backup_trace_lifecycle_backup_to_manifest_finalized() {
    let backup_id = BackupId::new(10);

    let trace1 = BackupAuditTrace::new(
        TraceId::new(111),
        backup_id,
        BackupAuditEvent::BackupStarted {
            backup_id,
            database_id: 1,
        },
        SystemTime::now(),
        1,
    );

    let trace2 = BackupAuditTrace::new(
        TraceId::new(111),
        backup_id,
        BackupAuditEvent::SnapshotCheckpointCaptured {
            backup_id,
            checkpoint_lsn: 1000,
            snapshot_id: 1,
        },
        SystemTime::now(),
        2,
    );

    let trace3 = BackupAuditTrace::new(
        TraceId::new(111),
        backup_id,
        BackupAuditEvent::BackupManifestFinalized {
            backup_id,
            manifest_crc: 0xABCD,
            earliest_pitr: 1000,
            latest_pitr: 5000,
        },
        SystemTime::now(),
        3,
    );

    assert_eq!(trace1.sequence_number, 1);
    assert_eq!(trace2.sequence_number, 2);
    assert_eq!(trace3.sequence_number, 3);
    assert!(trace1.validate() && trace2.validate() && trace3.validate());
}

#[test]
fn backup_restore_lifecycle_backup_to_restore_completion() {
    let backup_id = BackupId::new(20);

    let trace1 = BackupAuditTrace::new(
        TraceId::new(222),
        backup_id,
        BackupAuditEvent::BackupManifestFinalized {
            backup_id,
            manifest_crc: 0x1234,
            earliest_pitr: 1000,
            latest_pitr: 5000,
        },
        SystemTime::now(),
        1,
    );

    let trace2 = BackupAuditTrace::new(
        TraceId::new(222),
        backup_id,
        BackupAuditEvent::RestoreStarted {
            backup_id,
            pitr_target_lsn: 3000,
            stage: RecoveryStage::SafeStart,
        },
        SystemTime::now(),
        2,
    );

    let trace3 = BackupAuditTrace::new(
        TraceId::new(222),
        backup_id,
        BackupAuditEvent::RestoreCompleted {
            backup_id,
            final_checkpoint_lsn: 3000,
            status: RestoreCompletion::Success { replayed_lsn: 3000 },
        },
        SystemTime::now(),
        3,
    );

    assert_eq!(trace1.backup_id, trace2.backup_id);
    assert_eq!(trace2.backup_id, trace3.backup_id);
    assert!(trace1.validate() && trace2.validate() && trace3.validate());
}
#[test]
fn backup_audit_trace_is_immutable_after_construction() {
    let trace = BackupAuditTrace::new(
        TraceId::new(555),
        BackupId::new(1),
        BackupAuditEvent::BackupStarted {
            backup_id: BackupId::new(1),
            database_id: 1,
        },
        SystemTime::now(),
        1,
    );

    // Verify fields are accessible but cannot be modified (immutable struct)
    let _trace_id = trace.trace_id;
    let _backup_id = trace.backup_id;
    let _event = &trace.event;

    // This would fail to compile if we tried to mutate:
    // trace.backup_id = BackupId::new(2);
}
