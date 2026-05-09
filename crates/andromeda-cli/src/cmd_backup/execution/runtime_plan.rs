use crate::error::cli_error;
use andromeda_backup::{
    BackupExecutionPlan, BackupId, BackupManifest, BackupResourceLimits, BackupStorageTier,
    ColdSnapshotBoundary, ExtentCopyTask, FileBackedBackupArtifactStore, WAL_FORMAT_VERSION,
    WalArchiveRange, WalSegmentCopyTask,
};
use andromeda_core::AndromedaResult;
use andromeda_segment::{ExtentDescriptor, ExtentState, SegmentId};
use andromeda_storage_page::{AllocationId, ExtentId, ObjectId, PageId, PageSize};
use andromeda_wal::{Lsn, WalSegmentDescriptor};
use std::time::{SystemTime, UNIX_EPOCH};

const RUNTIME_SNAPSHOT_BYTES: &[u8] = b"andromeda-cli file-backed backup snapshot fixture v1";
const RUNTIME_WAL_SEGMENT_0_BYTES: &[u8] = b"andromeda-cli file-backed backup wal segment 0 v1";
const RUNTIME_WAL_SEGMENT_1_BYTES: &[u8] = b"andromeda-cli file-backed backup wal segment 1 v1";

pub(super) struct RuntimeBackupExecutionReport {
    pub(super) backup_id: u64,
    pub(super) artifact_root: String,
    pub(super) manifest_path: String,
    pub(super) snapshot_path: String,
    pub(super) wal_segment_paths: Vec<String>,
    pub(super) base_lsn: u64,
    pub(super) end_lsn: u64,
}

pub(super) fn execute_file_backed_backup(
    artifact_dir: &str,
    backup_id: u64,
) -> AndromedaResult<RuntimeBackupExecutionReport> {
    let plan = build_runtime_backup_execution_plan(backup_id)?;
    let store = FileBackedBackupArtifactStore::open(artifact_dir).map_err(|error| {
        cli_error(format!(
            "failed to open backup artifact directory `{artifact_dir}`: {}",
            error.message()
        ))
    })?;
    let wal_segments = [RUNTIME_WAL_SEGMENT_0_BYTES, RUNTIME_WAL_SEGMENT_1_BYTES];
    let report = store
        .write_execution_plan_artifact(&plan, RUNTIME_SNAPSHOT_BYTES, &wal_segments)
        .map_err(|error| {
            cli_error(format!(
                "failed to write backup execution plan artifact: {}",
                error.message()
            ))
        })?;

    Ok(RuntimeBackupExecutionReport {
        backup_id: report.backup_id.get(),
        artifact_root: report.artifact_root.display().to_string(),
        manifest_path: report.manifest_path.display().to_string(),
        snapshot_path: report.snapshot_path.display().to_string(),
        wal_segment_paths: report
            .wal_segment_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        base_lsn: plan.manifest.wal_archive.start.get(),
        end_lsn: plan.manifest.wal_archive.end_inclusive.get(),
    })
}

pub(super) fn generate_backup_id() -> u64 {
    unix_timestamp().max(1)
}

fn build_runtime_backup_execution_plan(backup_id: u64) -> AndromedaResult<BackupExecutionPlan> {
    let manifest = BackupManifest {
        backup_id: BackupId::new(backup_id),
        database_id: 42,
        created_epoch: unix_timestamp().max(1),
        snapshot: ColdSnapshotBoundary {
            snapshot_id: 99,
            snapshot_descriptor_hash: [0x5A; 32],
            base_checkpoint_lsn: Lsn::new(1000),
            required_wal_start_lsn: Lsn::new(1001),
        },
        wal_archive: WalArchiveRange::new(Lsn::new(1001), Lsn::new(2000)),
        manifest_crc: 777,
    };

    let extent = ExtentDescriptor {
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
    };
    let wal_segment_0 = WalSegmentDescriptor {
        format_version: WAL_FORMAT_VERSION,
        segment_id: 10,
        first_lsn: Lsn::new(1001),
        last_lsn: Lsn::new(1500),
        base_previous_lsn: None,
        record_count: 500,
    };
    let wal_segment_1 = WalSegmentDescriptor {
        format_version: WAL_FORMAT_VERSION,
        segment_id: 11,
        first_lsn: Lsn::new(1501),
        last_lsn: Lsn::new(2000),
        base_previous_lsn: Some(Lsn::new(1500)),
        record_count: 500,
    };

    let total_wal_bytes =
        (RUNTIME_WAL_SEGMENT_0_BYTES.len() + RUNTIME_WAL_SEGMENT_1_BYTES.len()) as u64;
    let plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: BackupStorageTier::ColdStore,
            byte_count: RUNTIME_SNAPSHOT_BYTES.len() as u64,
        }],
        wal_segment_copy_plan: vec![
            WalSegmentCopyTask {
                segment_descriptor: wal_segment_0,
                byte_count: RUNTIME_WAL_SEGMENT_0_BYTES.len() as u64,
                sequence_index: 0,
            },
            WalSegmentCopyTask {
                segment_descriptor: wal_segment_1,
                byte_count: RUNTIME_WAL_SEGMENT_1_BYTES.len() as u64,
                sequence_index: 1,
            },
        ],
        validate_checksum_on_copy: true,
        resource_limits: BackupResourceLimits {
            max_total_extent_bytes: 1_000_000,
            max_total_wal_bytes: 1_000_000,
            max_parallel_extent_tasks: 8,
            max_wal_segment_count: 16,
        },
        total_extent_bytes: RUNTIME_SNAPSHOT_BYTES.len() as u64,
        total_wal_bytes,
    };
    plan.validate().map_err(|error| {
        cli_error(format!(
            "failed to validate generated backup execution plan: {}",
            error.message()
        ))
    })?;
    Ok(plan)
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
