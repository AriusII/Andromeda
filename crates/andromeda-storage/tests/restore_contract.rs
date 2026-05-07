//! F6 Restore and PITR Contract Tests
//!
//! Validates RestoreOrchestrator type and RestorePipeline functions against
//! the durable backup contract and WAL replay semantics.

use andromeda_observe::TraceId;
use andromeda_storage::{
    AllocationId, BackupExecutionPlan, BackupResourceLimits, ExtentCopyTask, ExtentDescriptor,
    ExtentId, ExtentState, FileBackedBackupArtifactStore, Lsn, ObjectId, PageId, PageSize,
    PitrTarget, RecoveryStage, RestoreAuditTrace, RestoreCompletion, RestoreOrchestration,
    RestoreValidationPolicy, SegmentId, StorageTier, WalSegmentCopyTask, WalSegmentDescriptor,
    backup::{BackupId, BackupManifest, ColdSnapshotBoundary, WalArchiveRange},
    compute_restore_checksum, plan_replay_segments, validate_pitr_target,
    validate_restore_artifact_preflight, validate_restore_prerequisites,
};
use sha2::{Digest, Sha256};

const ARTIFACT_MANIFEST_MAGIC: &[u8] = b"ANDROMEDA-BACKUP-ARTIFACT-V1\n";
const ARTIFACT_MANIFEST_HEADER_LEN: usize = ARTIFACT_MANIFEST_MAGIC.len() + 2 + 8 + 32;
const CURRENT_ARTIFACT_MANIFEST_FORMAT_VERSION: u16 = 3;
const LEGACY_V1_ARTIFACT_MANIFEST_FORMAT_VERSION: u16 = 1;
const WAL_ARCHIVE_DIGEST_PAYLOAD_OFFSET: usize = 140;
const COMPATIBILITY_EVIDENCE_PAYLOAD_LEN: usize = 8;

// Test Fixtures

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

fn make_extent() -> ExtentDescriptor {
    ExtentDescriptor {
        extent_id: ExtentId::new(1),
        object_id: ObjectId::new(10),
        allocation_id: AllocationId::new(20),
        first_page_id: PageId::new(100),
        page_count: 1,
        page_size: PageSize::KiB16,
        state: ExtentState::PublishedCold,
        segment_id: Some(SegmentId::new(1)),
        file_offset: 0,
        allocated_on_disk: true,
    }
}

fn make_resource_limits() -> BackupResourceLimits {
    BackupResourceLimits {
        max_total_extent_bytes: 1_000_000,
        max_total_wal_bytes: 1_000_000,
        max_parallel_extent_tasks: 8,
        max_wal_segment_count: 16,
    }
}

fn write_test_artifact(
    temp: &tempfile::TempDir,
    backup_id: BackupId,
) -> andromeda_storage::BackupArtifactWriteReport {
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();
    let snapshot_bytes = b"restore preflight snapshot artifact";
    let wal_bytes = b"restore preflight wal segment";
    let mut manifest = make_test_manifest();
    manifest.backup_id = backup_id;
    let wal_segment = make_wal_segment(1001, 2000, None);

    let plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: make_extent(),
            source_tier: StorageTier::ColdStore,
            byte_count: snapshot_bytes.len() as u64,
        }],
        wal_segment_copy_plan: vec![WalSegmentCopyTask {
            segment_descriptor: wal_segment,
            byte_count: wal_bytes.len() as u64,
            sequence_index: 0,
        }],
        validate_checksum_on_copy: true,
        resource_limits: make_resource_limits(),
        total_extent_bytes: snapshot_bytes.len() as u64,
        total_wal_bytes: wal_bytes.len() as u64,
    };

    store
        .write_execution_plan_artifact(&plan, snapshot_bytes, &[wal_bytes.as_slice()])
        .unwrap()
}

fn rewrite_manifest_payload(
    path: &std::path::Path,
    mutate_payload: impl FnOnce(&mut Vec<u8>),
    format_version: u16,
) {
    let mut bytes = std::fs::read(path).unwrap();
    assert_eq!(
        &bytes[..ARTIFACT_MANIFEST_MAGIC.len()],
        ARTIFACT_MANIFEST_MAGIC
    );
    let mut payload = bytes[ARTIFACT_MANIFEST_HEADER_LEN..].to_vec();
    mutate_payload(&mut payload);

    bytes.truncate(ARTIFACT_MANIFEST_HEADER_LEN);
    let version_offset = ARTIFACT_MANIFEST_MAGIC.len();
    bytes[version_offset..version_offset + 2].copy_from_slice(&format_version.to_le_bytes());
    let payload_len_offset = version_offset + 2;
    bytes[payload_len_offset..payload_len_offset + 8]
        .copy_from_slice(&(payload.len() as u64).to_le_bytes());
    let checksum_offset = payload_len_offset + 8;
    let checksum: [u8; 32] = Sha256::digest(&payload).into();
    bytes[checksum_offset..checksum_offset + 32].copy_from_slice(&checksum);
    bytes.extend_from_slice(&payload);
    std::fs::write(path, bytes).unwrap();
}

fn rewrite_manifest_to_v1_without_archive_digest(path: &std::path::Path) {
    rewrite_manifest_payload(
        path,
        |payload| {
            payload
                .drain(WAL_ARCHIVE_DIGEST_PAYLOAD_OFFSET..WAL_ARCHIVE_DIGEST_PAYLOAD_OFFSET + 32);
            let legacy_len = payload.len() - COMPATIBILITY_EVIDENCE_PAYLOAD_LEN;
            payload.truncate(legacy_len);
        },
        LEGACY_V1_ARTIFACT_MANIFEST_FORMAT_VERSION,
    );
}

fn zero_manifest_archive_digest(path: &std::path::Path) {
    rewrite_manifest_payload(
        path,
        |payload| {
            payload[WAL_ARCHIVE_DIGEST_PAYLOAD_OFFSET..WAL_ARCHIVE_DIGEST_PAYLOAD_OFFSET + 32]
                .fill(0);
        },
        CURRENT_ARTIFACT_MANIFEST_FORMAT_VERSION,
    );
}

// Test Suite: PITR LSN Validation

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
fn test_pitr_lsn_at_snapshot_base_checkpoint_is_snapshot_only() {
    let manifest = make_test_manifest();
    let pitr_lsn = Lsn::new(1000);

    assert!(validate_restore_prerequisites(&manifest, pitr_lsn).is_ok());
    let plan = plan_replay_segments(&manifest, pitr_lsn, &[])
        .expect("snapshot base checkpoint target must not require WAL replay");
    assert!(plan.is_empty());
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
    let pitr_lsn = Lsn::new(999); // Below snapshot base checkpoint

    assert!(validate_restore_prerequisites(&manifest, pitr_lsn).is_err());
}

#[test]
fn test_pitr_lsn_above_range() {
    let manifest = make_test_manifest();
    let pitr_lsn = Lsn::new(2001); // Above archive end

    assert!(validate_restore_prerequisites(&manifest, pitr_lsn).is_err());
}

#[test]
fn test_restore_prerequisites_reject_wal_archive_after_snapshot_required_start() {
    let mut manifest = make_test_manifest();
    manifest.wal_archive = WalArchiveRange::new(Lsn::new(1002), Lsn::new(2000));

    let err = validate_restore_prerequisites(&manifest, Lsn::new(1500))
        .expect_err("archive must anchor at snapshot required WAL start");

    assert!(
        err.message().contains("required WAL start"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_restore_prerequisites_reject_pitr_before_snapshot_required_start() {
    let mut manifest = make_test_manifest();
    manifest.snapshot.required_wal_start_lsn = Lsn::new(1005);
    manifest.wal_archive = WalArchiveRange::new(Lsn::new(900), Lsn::new(2000));

    let err = validate_restore_prerequisites(&manifest, Lsn::new(1002))
        .expect_err("PITR before snapshot required WAL start must fail closed");

    assert!(
        err.message().contains("required WAL start"),
        "unexpected error: {err}"
    );
}

#[test]
fn backup_pitr_validator_rejects_target_before_required_wal_start() {
    let mut manifest = make_test_manifest();
    manifest.snapshot.required_wal_start_lsn = Lsn::new(1005);
    manifest.wal_archive = WalArchiveRange::new(Lsn::new(900), Lsn::new(2000));

    let err = validate_pitr_target(&manifest, PitrTarget::new(Lsn::new(1002)))
        .expect_err("PITR before required WAL start must fail closed");

    assert!(
        err.message().contains("required WAL start"),
        "unexpected error: {err}"
    );
}

// Test Suite: WAL Segment Replay Planning

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
    assert_eq!(segments_to_replay[0].replay_stop_lsn, pitr_lsn);
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
    assert_eq!(segments_to_replay[0].replay_stop_lsn, Lsn::new(1500));
    assert_eq!(segments_to_replay[1].replay_stop_lsn, pitr_lsn);
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
    assert_eq!(segments_to_replay[0].replay_stop_lsn, pitr_lsn);
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
fn test_plan_replay_requires_first_segment_at_archive_start() {
    let manifest = make_test_manifest();
    let segments = vec![make_wal_segment(1200, 2000, None)];
    let pitr_lsn = Lsn::new(1500);

    let err = plan_replay_segments(&manifest, pitr_lsn, &segments)
        .expect_err("restore must not skip WAL before the first segment");

    assert!(
        err.message().contains("archive start"),
        "unexpected error: {err}"
    );
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

// Test Suite: Restore Checksum Computation

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

// Test Suite: Audit Trace Validation

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

// Test Suite: RestoreOrchestration

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
fn test_restore_orchestration_rejects_audit_target_mismatch() {
    let manifest = make_test_manifest();
    let audit = RestoreAuditTrace::new(
        TraceId::new(1),
        BackupId::new(1),
        Lsn::new(1501),
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
    let audit = RestoreAuditTrace::new(
        TraceId::new(1),
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
    assert!(!orch.recovery_stage.application_traffic_allowed());
    assert!(!orch.recovery_stage.durable_truth_mutation_allowed());
    assert!(!orch.recovery_stage.in_place_repair_allowed());
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

#[test]
fn test_forensic_start_rejects_minimal_validation_policy() {
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

// Integration Tests: PITR Checkpoint Reconstruction

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
    assert_eq!(plan[0].replay_stop_lsn, pitr_lsn);
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

    assert_eq!(preflight.manifest_format_version, 1);
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
    let audit = RestoreAuditTrace::new(
        TraceId::new(1),
        backup_id,
        Lsn::new(1500),
        RecoveryStage::ForensicStart,
        preflight.restore_evidence_checksum,
    );

    let orchestration = RestoreOrchestration::new(
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
    let audit = RestoreAuditTrace::new(
        TraceId::new(1),
        backup_id,
        Lsn::new(1500),
        RecoveryStage::ForensicStart,
        preflight.restore_evidence_checksum.wrapping_add(1),
    );

    let orchestration = RestoreOrchestration::new(
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
