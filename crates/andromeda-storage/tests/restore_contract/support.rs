#[allow(dead_code)]
#[path = "../backup_execution_plan/support.rs"]
mod backup_support;

pub(crate) use backup_support::{
    CURRENT_ARTIFACT_MANIFEST_FORMAT_VERSION, LEGACY_V1_ARTIFACT_MANIFEST_FORMAT_VERSION,
    rewrite_manifest_to_v1_without_archive_digest, zero_manifest_archive_digest,
};

use andromeda_observe::TraceId;
use andromeda_storage::{
    BackupArtifactWriteReport, BackupId, BackupManifest, ColdSnapshotBoundary, ExtentState, Lsn,
    RecoveryStage, RestoreAuditTrace, RestoreOrchestration, RestoreValidationPolicy,
    WalArchiveRange, WalSegmentDescriptor, compute_restore_checksum,
};

pub(crate) fn make_test_manifest() -> BackupManifest {
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

pub(crate) fn make_wal_segment(
    first_lsn: u64,
    last_lsn: u64,
    base_previous_lsn: Option<u64>,
) -> WalSegmentDescriptor {
    backup_support::test_wal_segment(1, first_lsn, last_lsn, base_previous_lsn)
}

pub(crate) fn restore_audit_with_checksum(
    backup_id: BackupId,
    pitr_target_lsn: Lsn,
    stage: RecoveryStage,
    checksum: u64,
) -> RestoreAuditTrace {
    RestoreAuditTrace::new(TraceId::new(1), backup_id, pitr_target_lsn, stage, checksum)
}

pub(crate) fn restore_audit_for(
    manifest: &BackupManifest,
    pitr_target_lsn: Lsn,
    stage: RecoveryStage,
) -> RestoreAuditTrace {
    restore_audit_with_checksum(
        manifest.backup_id,
        pitr_target_lsn,
        stage,
        compute_restore_checksum(manifest),
    )
}

pub(crate) fn restore_orchestration_for(
    manifest: BackupManifest,
    pitr_target_lsn: Lsn,
    stage: RecoveryStage,
    validation_policy: RestoreValidationPolicy,
    audit: RestoreAuditTrace,
) -> RestoreOrchestration {
    RestoreOrchestration::new(manifest, pitr_target_lsn, stage, validation_policy, audit)
}

pub(crate) fn write_test_artifact(
    temp: &tempfile::TempDir,
    backup_id: BackupId,
) -> BackupArtifactWriteReport {
    let store = andromeda_storage::FileBackedBackupArtifactStore::open(temp.path()).unwrap();
    let snapshot_bytes = b"restore preflight snapshot artifact";
    let wal_bytes = b"restore preflight wal segment";
    let mut manifest = make_test_manifest();
    manifest.backup_id = backup_id;
    let wal_segment = make_wal_segment(1001, 2000, None);

    let plan = backup_support::backup_plan(
        manifest,
        vec![backup_support::cold_extent_copy_task(
            backup_support::test_extent(1, 100, 1, ExtentState::PublishedCold),
            snapshot_bytes.len() as u64,
        )],
        vec![backup_support::wal_segment_copy_task(
            wal_segment,
            wal_bytes.len() as u64,
            0,
        )],
        true,
        snapshot_bytes.len() as u64,
        wal_bytes.len() as u64,
    );

    store
        .write_execution_plan_artifact(&plan, snapshot_bytes, &[wal_bytes.as_slice()])
        .unwrap()
}
