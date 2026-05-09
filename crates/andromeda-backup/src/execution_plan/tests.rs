use super::*;
use crate::{BackupId, ColdSnapshotBoundary, WalArchiveRange};
use andromeda_segment::{
    AllocationId, ExtentDescriptor, ExtentId, ExtentState, ObjectId, PageId, PageSize, SegmentId,
};
use andromeda_wal::{Lsn, WalSegmentDescriptor};

fn sample_extent(extent_id: u64, first_page: u64, page_count: u32) -> ExtentDescriptor {
    ExtentDescriptor {
        extent_id: ExtentId::new(extent_id),
        object_id: ObjectId::new(1),
        allocation_id: AllocationId::new(1),
        first_page_id: PageId::new(first_page),
        page_count,
        page_size: PageSize::KiB16,
        state: ExtentState::PublishedCold,
        segment_id: Some(SegmentId::new(extent_id)),
        file_offset: 0,
        allocated_on_disk: false,
    }
}

fn sample_wal_segment(
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

fn sample_manifest() -> crate::BackupManifest<Lsn> {
    crate::BackupManifest {
        backup_id: BackupId::new(1),
        database_id: 1,
        created_epoch: 1000,
        snapshot: ColdSnapshotBoundary {
            snapshot_id: 1,
            snapshot_descriptor_hash: [1; 32],
            base_checkpoint_lsn: Lsn::new(100),
            required_wal_start_lsn: Lsn::new(101),
        },
        wal_archive: WalArchiveRange::new(Lsn::new(101), Lsn::new(200)),
        manifest_crc: 999,
    }
}

fn sample_resource_limits() -> BackupResourceLimits {
    BackupResourceLimits {
        max_total_extent_bytes: 1_000_000,
        max_total_wal_bytes: 500_000,
        max_parallel_extent_tasks: 8,
        max_wal_segment_count: 100,
    }
}

#[test]
fn backup_execution_plan_validates_manifest() {
    let mut manifest = sample_manifest();
    manifest.backup_id = BackupId::new(0);

    let extent = sample_extent(1, 100, 16);
    let wal_segment = sample_wal_segment(1, 101, 200, None);

    let plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: BackupStorageTier::ColdStore,
            byte_count: 64 * 1024,
        }],
        wal_segment_copy_plan: vec![WalSegmentCopyTask {
            segment_descriptor: wal_segment,
            byte_count: 100 * 1024,
            sequence_index: 0,
        }],
        validate_checksum_on_copy: false,
        resource_limits: sample_resource_limits(),
        total_extent_bytes: 64 * 1024,
        total_wal_bytes: 100 * 1024,
    };

    let result = plan.validate();
    assert!(result.is_err(), "plan should reject zero backup_id");
}

#[test]
fn backup_execution_plan_validates_extent_continuity() {
    let manifest = sample_manifest();
    let extent = sample_extent(1, 100, 16);

    let plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![
            ExtentCopyTask {
                extent_descriptor: extent,
                source_tier: BackupStorageTier::ColdStore,
                byte_count: 64 * 1024,
            },
            ExtentCopyTask {
                extent_descriptor: extent,
                source_tier: BackupStorageTier::ColdStore,
                byte_count: 64 * 1024,
            },
        ],
        wal_segment_copy_plan: vec![WalSegmentCopyTask {
            segment_descriptor: sample_wal_segment(1, 101, 200, None),
            byte_count: 100 * 1024,
            sequence_index: 0,
        }],
        validate_checksum_on_copy: false,
        resource_limits: sample_resource_limits(),
        total_extent_bytes: 128 * 1024,
        total_wal_bytes: 100 * 1024,
    };

    let result = plan.validate();
    assert!(result.is_err(), "plan should reject duplicate extent ids");
}

#[test]
fn backup_execution_plan_validates_wal_chain() {
    let manifest = sample_manifest();
    let extent = sample_extent(1, 100, 16);

    let plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: BackupStorageTier::ColdStore,
            byte_count: 64 * 1024,
        }],
        wal_segment_copy_plan: vec![
            WalSegmentCopyTask {
                segment_descriptor: sample_wal_segment(1, 101, 150, None),
                byte_count: 50 * 1024,
                sequence_index: 0,
            },
            WalSegmentCopyTask {
                segment_descriptor: sample_wal_segment(2, 152, 200, Some(150)),
                byte_count: 50 * 1024,
                sequence_index: 1,
            },
        ],
        validate_checksum_on_copy: false,
        resource_limits: sample_resource_limits(),
        total_extent_bytes: 64 * 1024,
        total_wal_bytes: 100 * 1024,
    };

    let result = plan.validate();
    assert!(
        result.is_err(),
        "plan should reject WAL segment chain with gap (expected LSN 151, got 152)"
    );
}

#[test]
fn backup_execution_plan_validates_resource_limits() {
    let manifest = sample_manifest();
    let extent = sample_extent(1, 100, 16);

    let limits = BackupResourceLimits {
        max_total_extent_bytes: 50 * 1024,
        max_total_wal_bytes: 500_000,
        max_parallel_extent_tasks: 8,
        max_wal_segment_count: 100,
    };

    let plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: BackupStorageTier::ColdStore,
            byte_count: 64 * 1024,
        }],
        wal_segment_copy_plan: vec![WalSegmentCopyTask {
            segment_descriptor: sample_wal_segment(1, 101, 200, None),
            byte_count: 100 * 1024,
            sequence_index: 0,
        }],
        validate_checksum_on_copy: false,
        resource_limits: limits,
        total_extent_bytes: 64 * 1024,
        total_wal_bytes: 100 * 1024,
    };

    let result = plan.validate();
    assert!(
        result.is_err(),
        "plan should reject extent bytes exceeding resource limit"
    );
}
