//! F6 Restore and PITR Contract Tests
//!
//! Validates RestoreOrchestrator type and RestorePipeline functions against
//! the durable backup contract and WAL replay semantics.

use andromeda_observe::TraceId;
use andromeda_storage::{
    Lsn, WalSegmentDescriptor,
    backup::{BackupId, BackupManifest, ColdSnapshotBoundary, WalArchiveRange},
    restore_orchestration::{
        RecoveryStage, RestoreAuditTrace, RestoreCompletion, RestoreOrchestration,
        RestoreValidationPolicy, compute_restore_checksum, plan_replay_segments,
        validate_restore_prerequisites,
    },
};

// ============================================================================
// Test Fixtures
// ============================================================================

fn make_test_manifest() -> BackupManifest {
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

fn make_wal_segment(
    first_lsn: u64,
    last_lsn: u64,
    base_previous_lsn: Option<u64>,
) -> WalSegmentDescriptor {
    WalSegmentDescriptor {
        format_version: 1,
        segment_id: 1,
        first_lsn: Lsn::new(first_lsn),
        last_lsn: Lsn::new(last_lsn),
        base_previous_lsn: base_previous_lsn.map(Lsn::new),
        record_count: (last_lsn - first_lsn + 1) as usize,
    }
}

// ============================================================================
// Test Suite: PITR LSN Validation
// ============================================================================

#[test]
fn test_pitr_lsn_within_range() {
    let manifest = make_test_manifest();
    let pitr_lsn = Lsn::new(1500);

    assert!(validate_restore_prerequisites(&manifest, pitr_lsn).is_ok());
}

#[test]
fn test_pitr_lsn_at_range_start() {
    let manifest = make_test_manifest();
    let pitr_lsn = Lsn::new(1001); // Exactly at start

    assert!(validate_restore_prerequisites(&manifest, pitr_lsn).is_ok());
}

#[test]
fn test_pitr_lsn_at_range_end() {
    let manifest = make_test_manifest();
    let pitr_lsn = Lsn::new(2000); // Exactly at end

    assert!(validate_restore_prerequisites(&manifest, pitr_lsn).is_ok());
}

#[test]
fn test_pitr_lsn_below_range() {
    let manifest = make_test_manifest();
    let pitr_lsn = Lsn::new(1000); // Below archive start

    assert!(validate_restore_prerequisites(&manifest, pitr_lsn).is_err());
}

#[test]
fn test_pitr_lsn_above_range() {
    let manifest = make_test_manifest();
    let pitr_lsn = Lsn::new(2001); // Above archive end

    assert!(validate_restore_prerequisites(&manifest, pitr_lsn).is_err());
}

// ============================================================================
// Test Suite: WAL Segment Replay Planning
// ============================================================================

#[test]
fn test_plan_replay_single_segment_containing_pitr() {
    let manifest = make_test_manifest();
    let segments = vec![make_wal_segment(1001, 2000, None)];
    let pitr_lsn = Lsn::new(1500);

    let plan = plan_replay_segments(&manifest, pitr_lsn, &segments);
    assert!(plan.is_ok());

    let segments_to_replay = plan.unwrap();
    assert_eq!(segments_to_replay.len(), 1);
    assert!(segments_to_replay[0].contains_pitr_target);
    assert_eq!(segments_to_replay[0].sequence_index, 0);
}

#[test]
fn test_plan_replay_multiple_segments() {
    let manifest = make_test_manifest();
    let segments = vec![
        make_wal_segment(1001, 1500, None),
        make_wal_segment(1501, 2000, Some(1500)),
    ];
    let pitr_lsn = Lsn::new(1750);

    let plan = plan_replay_segments(&manifest, pitr_lsn, &segments);
    assert!(plan.is_ok());

    let segments_to_replay = plan.unwrap();
    assert_eq!(segments_to_replay.len(), 2);
    assert!(!segments_to_replay[0].contains_pitr_target);
    assert!(segments_to_replay[1].contains_pitr_target);
}

#[test]
fn test_plan_replay_stops_after_pitr_segment() {
    let manifest = make_test_manifest();
    let segments = vec![
        make_wal_segment(1001, 1500, None),
        make_wal_segment(1501, 2000, Some(1500)),
    ];
    let pitr_lsn = Lsn::new(1250); // In first segment

    let plan = plan_replay_segments(&manifest, pitr_lsn, &segments);
    assert!(plan.is_ok());

    let segments_to_replay = plan.unwrap();
    assert_eq!(segments_to_replay.len(), 1);
    assert!(segments_to_replay[0].contains_pitr_target);
}

#[test]
fn test_plan_replay_empty_segments_rejected() {
    let manifest = make_test_manifest();
    let pitr_lsn = Lsn::new(1500);

    let plan = plan_replay_segments(&manifest, pitr_lsn, &[]);
    assert!(plan.is_err());
}

#[test]
fn test_plan_replay_detects_lsn_gap() {
    let manifest = make_test_manifest();
    // Create a gap: first segment ends at 1400, second starts at 1402
    let segments = vec![
        make_wal_segment(1001, 1400, None),
        make_wal_segment(1402, 2000, Some(1401)), // Gap!
    ];
    let pitr_lsn = Lsn::new(1500);

    let plan = plan_replay_segments(&manifest, pitr_lsn, &segments);
    assert!(plan.is_err());
}

#[test]
fn test_plan_replay_validates_first_segment_no_previous_lsn() {
    let manifest = make_test_manifest();
    // First segment has base_previous_lsn set (invalid)
    let segments = vec![make_wal_segment(1001, 1500, Some(1000))];
    let pitr_lsn = Lsn::new(1250);

    let plan = plan_replay_segments(&manifest, pitr_lsn, &segments);
    assert!(plan.is_err());
}

// ============================================================================
// Test Suite: Restore Checksum Computation
// ============================================================================

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

    // Modify backup ID
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

// ============================================================================
// Test Suite: Audit Trace Validation
// ============================================================================

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
        Some(RestoreCompletion::Success { .. }) => {
            // OK
        }
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
        }
        _ => panic!("Expected failure"),
    }
}

// ============================================================================
// Test Suite: RestoreOrchestration
// ============================================================================

#[test]
fn test_restore_orchestration_constructs_successfully() {
    let manifest = make_test_manifest();
    let audit = RestoreAuditTrace::new(
        TraceId::new(1),
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

    assert_eq!(orch.pitr_target_lsn, Lsn::new(1500));
    assert_eq!(orch.recovery_stage, RecoveryStage::SafeStart);
}

#[test]
fn test_restore_orchestration_validates_manifest() {
    let manifest = make_test_manifest();
    let audit = RestoreAuditTrace::new(
        TraceId::new(1),
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
fn test_restore_orchestration_rejects_pitr_out_of_range() {
    let manifest = make_test_manifest();
    let audit = RestoreAuditTrace::new(
        TraceId::new(1),
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

#[test]
fn test_restore_orchestration_supports_forensic_start() {
    let manifest = make_test_manifest();
    let audit = RestoreAuditTrace::new(
        TraceId::new(1),
        BackupId::new(1),
        Lsn::new(1500),
        RecoveryStage::ForensicStart,
        compute_restore_checksum(&manifest),
    );

    let orch = RestoreOrchestration::new(
        manifest,
        Lsn::new(1500),
        RecoveryStage::ForensicStart,
        RestoreValidationPolicy::Full,
        audit,
    );

    assert!(orch.validate().is_ok());
    assert_eq!(orch.recovery_stage, RecoveryStage::ForensicStart);
}

#[test]
fn test_restore_orchestration_supports_minimal_validation() {
    let manifest = make_test_manifest();
    let audit = RestoreAuditTrace::new(
        TraceId::new(1),
        BackupId::new(1),
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        compute_restore_checksum(&manifest),
    );

    let orch = RestoreOrchestration::new(
        manifest,
        Lsn::new(1500),
        RecoveryStage::SafeStart,
        RestoreValidationPolicy::Minimal,
        audit,
    );

    assert!(orch.validate().is_ok());
    assert_eq!(orch.validation_policy, RestoreValidationPolicy::Minimal);
}

// ============================================================================
// Integration Tests: PITR Checkpoint Reconstruction
// ============================================================================

#[test]
fn test_pitr_checkpoint_after_single_segment_replay() {
    let manifest = make_test_manifest();
    let segments = vec![make_wal_segment(1001, 2000, None)];
    let pitr_lsn = Lsn::new(1500);

    let plan = plan_replay_segments(&manifest, pitr_lsn, &segments).expect("plan failed");

    // Reconstruct checkpoint: should include all segments up to and including PITR
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].segment_descriptor.first_lsn, Lsn::new(1001));
    assert_eq!(plan[0].segment_descriptor.last_lsn, Lsn::new(2000));
    assert!(plan[0].contains_pitr_target);
}

#[test]
fn test_pitr_checkpoint_preserves_segment_chain() {
    let manifest = make_test_manifest();
    let segments = vec![
        make_wal_segment(1001, 1500, None),
        make_wal_segment(1501, 2000, Some(1500)),
    ];
    let pitr_lsn = Lsn::new(1750);

    let plan = plan_replay_segments(&manifest, pitr_lsn, &segments).expect("plan failed");

    // Should have both segments
    assert_eq!(plan.len(), 2);
    // Verify chain: first segment's last_lsn + 1 == second segment's first_lsn
    let seg1_end = plan[0].segment_descriptor.last_lsn;
    let seg2_start = plan[1].segment_descriptor.first_lsn;
    assert_eq!(seg1_end.get() + 1, seg2_start.get());
}
