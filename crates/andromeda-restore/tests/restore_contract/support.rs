#[allow(dead_code)]
#[path = "../backup_execution_plan/support.rs"]
mod backup_support;

use andromeda_backup::{
    BackupArtifactWriteReport, BackupId, BackupManifest as BackupManifestRaw, ColdSnapshotBoundary,
    FileBackedBackupArtifactStore, WalArchiveRange,
};
use andromeda_observability::TraceId;
use andromeda_restore::{
    RecoveryStage, RestoreAuditTrace, RestoreOrchestration, RestoreValidationPolicy,
    compute_restore_checksum,
};
use andromeda_segment::ExtentState;
use andromeda_wal::{Lsn, WalSegmentDescriptor};
use sha2::{Digest, Sha256};
use std::path::Path;

pub(crate) type BackupManifest = BackupManifestRaw<Lsn>;

const ARTIFACT_MANIFEST_MAGIC: &[u8] = b"ANDROMEDA-BACKUP-ARTIFACT-V1\n";
const ARTIFACT_MANIFEST_HEADER_LEN: usize = ARTIFACT_MANIFEST_MAGIC.len() + 2 + 8 + 32;
const WAL_ARCHIVE_DIGEST_PAYLOAD_OFFSET: usize = 140;
const CATALOG_VERSION_PAYLOAD_OFFSET: usize = 280;
const CATALOG_AUDIT_PAYLOAD_LEN: usize = 112;
const COMPATIBILITY_EVIDENCE_PAYLOAD_LEN: usize = 8;

pub(crate) const CURRENT_ARTIFACT_MANIFEST_FORMAT_VERSION: u16 = 4;
pub(crate) const LEGACY_V1_ARTIFACT_MANIFEST_FORMAT_VERSION: u16 =
    backup_support::LEGACY_V1_ARTIFACT_MANIFEST_FORMAT_VERSION;

pub(crate) fn rewrite_manifest_to_v1_without_archive_digest(path: &Path) {
    rewrite_manifest_payload(
        path,
        |payload| {
            payload
                .drain(WAL_ARCHIVE_DIGEST_PAYLOAD_OFFSET..WAL_ARCHIVE_DIGEST_PAYLOAD_OFFSET + 32);
            payload.drain(
                CATALOG_VERSION_PAYLOAD_OFFSET - 32
                    ..CATALOG_VERSION_PAYLOAD_OFFSET - 32 + CATALOG_AUDIT_PAYLOAD_LEN,
            );
            let legacy_len = payload.len() - COMPATIBILITY_EVIDENCE_PAYLOAD_LEN;
            payload.truncate(legacy_len);
        },
        LEGACY_V1_ARTIFACT_MANIFEST_FORMAT_VERSION,
    );
}

pub(crate) fn zero_manifest_archive_digest(path: &Path) {
    rewrite_manifest_payload(
        path,
        |payload| {
            payload[WAL_ARCHIVE_DIGEST_PAYLOAD_OFFSET..WAL_ARCHIVE_DIGEST_PAYLOAD_OFFSET + 32]
                .fill(0);
        },
        CURRENT_ARTIFACT_MANIFEST_FORMAT_VERSION,
    );
}

fn rewrite_manifest_payload(
    path: &Path,
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
    write_test_artifact_with_catalog_audit(
        temp,
        backup_id,
        b"restore preflight catalog artifact",
        b"restore preflight audit ledger artifact",
    )
}

pub(crate) fn write_test_artifact_with_catalog_audit(
    temp: &tempfile::TempDir,
    backup_id: BackupId,
    catalog_bytes: &[u8],
    audit_ledger_bytes: &[u8],
) -> BackupArtifactWriteReport {
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();
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
        .write_execution_plan_artifact_strict(
            &plan,
            snapshot_bytes,
            &[wal_bytes.as_slice()],
            catalog_bytes,
            audit_ledger_bytes,
        )
        .unwrap()
}
