use crate::support::*;

#[test]
fn backup_audit_event_backup_started_is_constructible() {
    let event = BackupAuditEvent::BackupStarted {
        backup_id: BackupId::new(1),
        database_id: 99,
    };

    assert_eq!(event.event_type(), "backup_started");
    assert_eq!(event.backup_id(), BackupId::new(1));
}

#[test]
fn backup_audit_event_snapshot_checkpoint_captured_is_constructible() {
    let event = BackupAuditEvent::SnapshotCheckpointCaptured {
        backup_id: BackupId::new(1),
        checkpoint_lsn: 1000,
        snapshot_id: 5,
    };

    assert_eq!(event.event_type(), "snapshot_checkpoint_captured");
    assert_eq!(event.backup_id(), BackupId::new(1));
}

#[test]
fn backup_audit_event_wal_segment_archived_is_constructible() {
    let event = BackupAuditEvent::WalSegmentArchived {
        backup_id: BackupId::new(1),
        segment_id: 42,
        lsn_range: (1000, 2000),
        checksum: 0xCAFEBABE,
    };

    assert_eq!(event.event_type(), "wal_segment_archived");
    assert_eq!(event.backup_id(), BackupId::new(1));
}

#[test]
fn backup_audit_event_manifest_finalized_is_constructible() {
    let event = BackupAuditEvent::BackupManifestFinalized {
        backup_id: BackupId::new(1),
        manifest_crc: 0x12345678,
        earliest_pitr: 1000,
        latest_pitr: 3000,
    };

    assert_eq!(event.event_type(), "backup_manifest_finalized");
    assert_eq!(event.backup_id(), BackupId::new(1));
}

#[test]
fn backup_audit_event_validation_failed_is_constructible() {
    let event = BackupAuditEvent::BackupValidationFailed {
        backup_id: BackupId::new(1),
        reason: "segment_checksum_mismatch".to_string(),
    };

    assert_eq!(event.event_type(), "backup_validation_failed");
    assert_eq!(event.backup_id(), BackupId::new(1));
}

#[test]
fn backup_audit_event_restore_started_is_constructible() {
    let event = BackupAuditEvent::RestoreStarted {
        backup_id: BackupId::new(1),
        pitr_target_lsn: 1500,
        stage: RecoveryStage::SafeStart,
    };

    assert_eq!(event.event_type(), "restore_started");
    assert_eq!(event.backup_id(), BackupId::new(1));
}

#[test]
fn backup_audit_event_restore_completed_success_is_constructible() {
    let event = BackupAuditEvent::RestoreCompleted {
        backup_id: BackupId::new(1),
        final_checkpoint_lsn: 1500,
        status: RestoreCompletion::Success { replayed_lsn: 1500 },
    };

    assert_eq!(event.event_type(), "restore_completed");
    assert_eq!(event.backup_id(), BackupId::new(1));
}

#[test]
fn backup_audit_event_restore_completed_failure_is_constructible() {
    let event = BackupAuditEvent::RestoreCompleted {
        backup_id: BackupId::new(1),
        final_checkpoint_lsn: 1000,
        status: RestoreCompletion::Failed {
            reason: "wal_gap_detected".to_string(),
        },
    };

    assert_eq!(event.event_type(), "restore_completed");
    assert_eq!(event.backup_id(), BackupId::new(1));
}

#[test]
fn backup_audit_event_backup_id_extraction_is_consistent_across_all_events() {
    let backup_id = BackupId::new(99);

    let events: Vec<BackupAuditEvent> = vec![
        BackupAuditEvent::BackupStarted {
            backup_id,
            database_id: 1,
        },
        BackupAuditEvent::SnapshotCheckpointCaptured {
            backup_id,
            checkpoint_lsn: 1000,
            snapshot_id: 1,
        },
        BackupAuditEvent::WalSegmentArchived {
            backup_id,
            segment_id: 1,
            lsn_range: (1000, 2000),
            checksum: 0,
        },
        BackupAuditEvent::BackupManifestFinalized {
            backup_id,
            manifest_crc: 0,
            earliest_pitr: 1000,
            latest_pitr: 2000,
        },
        BackupAuditEvent::BackupValidationFailed {
            backup_id,
            reason: "test".to_string(),
        },
        BackupAuditEvent::RestoreStarted {
            backup_id,
            pitr_target_lsn: 1500,
            stage: RecoveryStage::SafeStart,
        },
        BackupAuditEvent::RestoreCompleted {
            backup_id,
            final_checkpoint_lsn: 1500,
            status: RestoreCompletion::Success { replayed_lsn: 1500 },
        },
    ];

    for event in events {
        assert_eq!(event.backup_id(), backup_id);
    }
}
