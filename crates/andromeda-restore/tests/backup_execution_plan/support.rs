use andromeda_backup::{
    BackupExecutionPlan as BackupExecutionPlanRaw, BackupId, BackupManifest as BackupManifestRaw,
    BackupResourceLimits, BackupStorageTier, ColdSnapshotBoundary,
    ExtentCopyTask as ExtentCopyTaskRaw, WalArchiveRange, WalSegmentCopyTask,
};
use andromeda_segment::{
    AllocationId, ExtentDescriptor, ExtentId, ExtentState, ObjectId, PageId, PageSize, SegmentId,
};
use andromeda_wal::{Lsn, WalSegmentDescriptor};
use sha2::{Digest, Sha256};
use std::path::Path;

pub(crate) type BackupManifest = BackupManifestRaw<Lsn>;
pub(crate) type BackupExecutionPlan = BackupExecutionPlanRaw<BackupStorageTier>;
pub(crate) type ExtentCopyTask = ExtentCopyTaskRaw<BackupStorageTier>;

const ARTIFACT_MANIFEST_MAGIC: &[u8] = b"ANDROMEDA-BACKUP-ARTIFACT-V1\n";
const ARTIFACT_MANIFEST_HEADER_LEN: usize = ARTIFACT_MANIFEST_MAGIC.len() + 2 + 8 + 32;
const WAL_ARCHIVE_DIGEST_PAYLOAD_OFFSET: usize = 140;
const COLD_SNAPSHOT_MANIFEST_VERSION_PAYLOAD_OFFSET: usize = 180;
const COLD_SNAPSHOT_MANIFEST_CRC_PAYLOAD_OFFSET: usize = 228;
const WAL_SEGMENT_COUNT_PAYLOAD_OFFSET: usize = 280;
const COMPATIBILITY_EVIDENCE_PAYLOAD_LEN: usize = 8;

pub(crate) const CURRENT_ARTIFACT_MANIFEST_FORMAT_VERSION: u16 = 3;
pub(crate) const LEGACY_V2_ARTIFACT_MANIFEST_FORMAT_VERSION: u16 = 2;
pub(crate) const LEGACY_V1_ARTIFACT_MANIFEST_FORMAT_VERSION: u16 = 1;

pub(crate) fn test_extent(
    extent_id: u64,
    first_page: u64,
    page_count: u32,
    state: ExtentState,
) -> ExtentDescriptor {
    ExtentDescriptor {
        extent_id: ExtentId::new(extent_id),
        object_id: ObjectId::new(10 + extent_id),
        allocation_id: AllocationId::new(20 + extent_id),
        first_page_id: PageId::new(first_page),
        page_count,
        page_size: PageSize::KiB16,
        state,
        segment_id: Some(SegmentId::new(100 + extent_id)),
        file_offset: extent_id * 16384,
        allocated_on_disk: true,
    }
}

pub(crate) fn test_wal_segment(
    segment_id: u64,
    first_lsn: u64,
    last_lsn: u64,
    base_previous_lsn: Option<u64>,
) -> WalSegmentDescriptor {
    WalSegmentDescriptor {
        format_version: 1,
        segment_id,
        first_lsn: Lsn::new(first_lsn),
        last_lsn: Lsn::new(last_lsn),
        base_previous_lsn: base_previous_lsn.map(Lsn::new),
        record_count: (last_lsn - first_lsn + 1) as usize,
    }
}

pub(crate) fn test_manifest(backup_id: u64, wal_start: u64, wal_end: u64) -> BackupManifest {
    BackupManifest {
        backup_id: BackupId::new(backup_id),
        database_id: 42,
        created_epoch: 1000000,
        snapshot: ColdSnapshotBoundary {
            snapshot_id: 99,
            snapshot_descriptor_hash: [5; 32],
            base_checkpoint_lsn: Lsn::new(100),
            required_wal_start_lsn: Lsn::new(wal_start),
        },
        wal_archive: WalArchiveRange::new(Lsn::new(wal_start), Lsn::new(wal_end)),
        manifest_crc: 777,
    }
}

pub(crate) fn test_resource_limits() -> BackupResourceLimits {
    BackupResourceLimits {
        max_total_extent_bytes: 1_000_000_000,
        max_total_wal_bytes: 500_000_000,
        max_parallel_extent_tasks: 16,
        max_wal_segment_count: 1000,
    }
}

pub(crate) fn cold_extent_copy_task(
    extent_descriptor: ExtentDescriptor,
    byte_count: u64,
) -> ExtentCopyTask {
    ExtentCopyTask {
        extent_descriptor,
        source_tier: BackupStorageTier::ColdStore,
        byte_count,
    }
}

pub(crate) fn wal_segment_copy_task(
    segment_descriptor: WalSegmentDescriptor,
    byte_count: u64,
    sequence_index: usize,
) -> WalSegmentCopyTask {
    WalSegmentCopyTask {
        segment_descriptor,
        byte_count,
        sequence_index,
    }
}

pub(crate) fn backup_plan(
    manifest: BackupManifest,
    extent_copy_plan: Vec<ExtentCopyTask>,
    wal_segment_copy_plan: Vec<WalSegmentCopyTask>,
    validate_checksum_on_copy: bool,
    total_extent_bytes: u64,
    total_wal_bytes: u64,
) -> BackupExecutionPlan {
    BackupExecutionPlan {
        manifest,
        extent_copy_plan,
        wal_segment_copy_plan,
        validate_checksum_on_copy,
        resource_limits: test_resource_limits(),
        total_extent_bytes,
        total_wal_bytes,
    }
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

pub(crate) fn rewrite_manifest_to_v1_without_archive_digest(path: &Path) {
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

pub(crate) fn rewrite_manifest_to_v2_without_compatibility_evidence(path: &Path) {
    rewrite_manifest_payload(
        path,
        |payload| {
            let legacy_len = payload.len() - COMPATIBILITY_EVIDENCE_PAYLOAD_LEN;
            payload.truncate(legacy_len);
        },
        LEGACY_V2_ARTIFACT_MANIFEST_FORMAT_VERSION,
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

pub(crate) fn rewrite_cold_snapshot_manifest_crc(path: &Path, manifest_crc: u32) {
    rewrite_manifest_payload(
        path,
        |payload| {
            payload[COLD_SNAPSHOT_MANIFEST_CRC_PAYLOAD_OFFSET
                ..COLD_SNAPSHOT_MANIFEST_CRC_PAYLOAD_OFFSET + 4]
                .copy_from_slice(&manifest_crc.to_le_bytes());
        },
        CURRENT_ARTIFACT_MANIFEST_FORMAT_VERSION,
    );
}

pub(crate) fn rewrite_cold_snapshot_manifest_version(path: &Path, manifest_version: u64) {
    rewrite_manifest_payload(
        path,
        |payload| {
            payload[COLD_SNAPSHOT_MANIFEST_VERSION_PAYLOAD_OFFSET
                ..COLD_SNAPSHOT_MANIFEST_VERSION_PAYLOAD_OFFSET + 8]
                .copy_from_slice(&manifest_version.to_le_bytes());
        },
        CURRENT_ARTIFACT_MANIFEST_FORMAT_VERSION,
    );
}

pub(crate) fn rewrite_manifest_wal_segment_count(path: &Path, segment_count: u64) {
    rewrite_manifest_payload(
        path,
        |payload| {
            payload[WAL_SEGMENT_COUNT_PAYLOAD_OFFSET..WAL_SEGMENT_COUNT_PAYLOAD_OFFSET + 8]
                .copy_from_slice(&segment_count.to_le_bytes());
        },
        CURRENT_ARTIFACT_MANIFEST_FORMAT_VERSION,
    );
}

pub(crate) fn rewrite_compatibility_wal_format(path: &Path, wal_format_version: u16) {
    rewrite_manifest_payload(
        path,
        |payload| {
            let wal_format_offset = payload.len() - 2;
            payload[wal_format_offset..wal_format_offset + 2]
                .copy_from_slice(&wal_format_version.to_le_bytes());
        },
        CURRENT_ARTIFACT_MANIFEST_FORMAT_VERSION,
    );
}
