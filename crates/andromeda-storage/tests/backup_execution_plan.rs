//! Physical backup execution plan contract tests.
//!
//! # Test Scenarios
//!
//! 1. **test_backup_plan_validates_manifest** — Create snapshot manifest, call backup planner,
//!    verify artifact structure and that invalid manifests are rejected.
//!
//! 2. **test_backup_plan_includes_wal_range** — Backup plan includes all durable WAL segments
//!    in range [wal_start_lsn, wal_end_lsn] with no gaps.
//!
//! 3. **test_backup_artifact_checksum** — Backup artifact metadata has checksum; corrupting
//!    one field invalidates it.
//!
//! 4. **test_backup_rejects_incomplete_wal** — If WAL segment is missing from the range,
//!    backup plan should raise validation error.

use andromeda_storage::{
    AllocationId, BackupExecutionPlan, BackupId, BackupManifest, BackupResourceLimits,
    ColdSnapshotBoundary, ExtentCopyTask, ExtentDescriptor, ExtentId, ExtentState,
    FileBackedBackupArtifactStore, Lsn, ObjectId, PageId, PageSize, SegmentId, StorageTier,
    WalArchiveRange, WalSegmentCopyTask, WalSegmentDescriptor,
};
use sha2::{Digest, Sha256};

const ARTIFACT_MANIFEST_MAGIC: &[u8] = b"ANDROMEDA-BACKUP-ARTIFACT-V1\n";
const ARTIFACT_MANIFEST_HEADER_LEN: usize = ARTIFACT_MANIFEST_MAGIC.len() + 2 + 8 + 32;
const CURRENT_ARTIFACT_MANIFEST_FORMAT_VERSION: u16 = 3;
const LEGACY_V2_ARTIFACT_MANIFEST_FORMAT_VERSION: u16 = 2;
const LEGACY_V1_ARTIFACT_MANIFEST_FORMAT_VERSION: u16 = 1;
const WAL_ARCHIVE_DIGEST_PAYLOAD_OFFSET: usize = 140;
const COLD_SNAPSHOT_MANIFEST_VERSION_PAYLOAD_OFFSET: usize = 180;
const COLD_SNAPSHOT_MANIFEST_CRC_PAYLOAD_OFFSET: usize = 228;
const WAL_SEGMENT_COUNT_PAYLOAD_OFFSET: usize = 280;
const COMPATIBILITY_EVIDENCE_PAYLOAD_LEN: usize = 8;

/// Helper: Create a sample extent descriptor for testing.
fn test_extent(
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

/// Helper: Create a sample WAL segment descriptor for testing.
fn test_wal_segment(
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

/// Helper: Create a sample manifest for testing.
fn test_manifest(backup_id: u64, wal_start: u64, wal_end: u64) -> BackupManifest {
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

/// Helper: Create default resource limits.
fn test_resource_limits() -> BackupResourceLimits {
    BackupResourceLimits {
        max_total_extent_bytes: 1_000_000_000,
        max_total_wal_bytes: 500_000_000,
        max_parallel_extent_tasks: 16,
        max_wal_segment_count: 1000,
    }
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

fn rewrite_manifest_to_v2_without_compatibility_evidence(path: &std::path::Path) {
    rewrite_manifest_payload(
        path,
        |payload| {
            let legacy_len = payload.len() - COMPATIBILITY_EVIDENCE_PAYLOAD_LEN;
            payload.truncate(legacy_len);
        },
        LEGACY_V2_ARTIFACT_MANIFEST_FORMAT_VERSION,
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

fn rewrite_cold_snapshot_manifest_crc(path: &std::path::Path, manifest_crc: u32) {
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

fn rewrite_cold_snapshot_manifest_version(path: &std::path::Path, manifest_version: u64) {
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

fn rewrite_manifest_wal_segment_count(path: &std::path::Path, segment_count: u64) {
    rewrite_manifest_payload(
        path,
        |payload| {
            payload[WAL_SEGMENT_COUNT_PAYLOAD_OFFSET..WAL_SEGMENT_COUNT_PAYLOAD_OFFSET + 8]
                .copy_from_slice(&segment_count.to_le_bytes());
        },
        CURRENT_ARTIFACT_MANIFEST_FORMAT_VERSION,
    );
}

fn rewrite_compatibility_wal_format(path: &std::path::Path, wal_format_version: u16) {
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

#[test]
fn test_backup_plan_validates_manifest() {
    let manifest = test_manifest(1, 101, 300);

    let extent1 = test_extent(1, 1000, 100, ExtentState::PublishedCold);
    let extent2 = test_extent(2, 1100, 50, ExtentState::PublishedCold);

    let wal_seg1 = test_wal_segment(1, 101, 200, None);
    let wal_seg2 = test_wal_segment(2, 201, 300, Some(200));

    let extent1_bytes = 100 * 16 * 1024; // 100 pages * 16KB/page
    let extent2_bytes = 50 * 16 * 1024; // 50 pages * 16KB/page
    let wal1_bytes = 100 * 1024;
    let wal2_bytes = 100 * 1024;

    let plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![
            ExtentCopyTask {
                extent_descriptor: extent1,
                source_tier: StorageTier::ColdStore,
                byte_count: extent1_bytes,
            },
            ExtentCopyTask {
                extent_descriptor: extent2,
                source_tier: StorageTier::ColdStore,
                byte_count: extent2_bytes,
            },
        ],
        wal_segment_copy_plan: vec![
            WalSegmentCopyTask {
                segment_descriptor: wal_seg1,
                byte_count: wal1_bytes,
                sequence_index: 0,
            },
            WalSegmentCopyTask {
                segment_descriptor: wal_seg2,
                byte_count: wal2_bytes,
                sequence_index: 1,
            },
        ],
        validate_checksum_on_copy: true,
        resource_limits: test_resource_limits(),
        total_extent_bytes: extent1_bytes + extent2_bytes,
        total_wal_bytes: wal1_bytes + wal2_bytes,
    };

    // Plan should validate successfully.
    assert!(
        plan.validate().is_ok(),
        "valid backup execution plan should pass validation"
    );

    let bad_manifest = BackupManifest {
        backup_id: BackupId::new(0), // Invalid!
        ..manifest
    };

    let bad_plan = BackupExecutionPlan {
        manifest: bad_manifest,
        ..plan.clone()
    };

    assert!(
        bad_plan.validate().is_err(),
        "backup plan with zero backup_id should fail validation"
    );

    let bad_manifest = BackupManifest {
        manifest_crc: 0, // Invalid!
        ..manifest
    };

    let bad_plan = BackupExecutionPlan {
        manifest: bad_manifest,
        ..plan.clone()
    };

    assert!(
        bad_plan.validate().is_err(),
        "backup plan with zero manifest_crc should fail validation"
    );

    let mutable_extent = test_extent(3, 1200, 25, ExtentState::AllocatingHot);

    let bad_plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: mutable_extent,
            source_tier: StorageTier::ColdStore,
            byte_count: 25 * 4096,
        }],
        ..plan
    };

    assert!(
        bad_plan.validate().is_err(),
        "backup plan with mutable extent should fail validation"
    );
}

#[test]
fn test_backup_plan_includes_wal_range() {
    // Test: Backup plan includes all durable WAL segments in range [wal_start_lsn, wal_end_lsn]
    // with no gaps and contiguous LSN coverage.

    let wal_start = 1001;
    let wal_end = 3000;

    let manifest = test_manifest(2, wal_start, wal_end);

    let extent = test_extent(1, 1000, 100, ExtentState::PublishedCold);

    // Create a multi-segment WAL archive that covers the range exactly.
    let wal_seg1 = test_wal_segment(10, wal_start, 1500, None);
    let wal_seg2 = test_wal_segment(11, 1501, 2500, Some(1500));
    let wal_seg3 = test_wal_segment(12, 2501, wal_end, Some(2500));

    let plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: StorageTier::ColdStore,
            byte_count: 100 * 4096,
        }],
        wal_segment_copy_plan: vec![
            WalSegmentCopyTask {
                segment_descriptor: wal_seg1,
                byte_count: 500 * 1024,
                sequence_index: 0,
            },
            WalSegmentCopyTask {
                segment_descriptor: wal_seg2,
                byte_count: 1000 * 1024,
                sequence_index: 1,
            },
            WalSegmentCopyTask {
                segment_descriptor: wal_seg3,
                byte_count: 500 * 1024,
                sequence_index: 2,
            },
        ],
        validate_checksum_on_copy: false,
        resource_limits: test_resource_limits(),
        total_extent_bytes: 100 * 4096,
        total_wal_bytes: 2000 * 1024,
    };

    // Plan should validate successfully and cover the full range.
    assert!(
        plan.validate().is_ok(),
        "backup plan with contiguous WAL segments should pass validation"
    );

    assert_eq!(
        plan.wal_segment_count(),
        3,
        "plan should include all 3 WAL segments"
    );

    // Verify each LSN in the range is covered.
    assert!(
        plan.covers_wal_lsn(Lsn::new(wal_start)),
        "plan should cover WAL start LSN"
    );
    assert!(
        plan.covers_wal_lsn(Lsn::new(2000)),
        "plan should cover LSN in middle of range"
    );
    assert!(
        plan.covers_wal_lsn(Lsn::new(wal_end)),
        "plan should cover WAL end LSN"
    );

    // Test: WAL archive that doesn't cover the full range (ends before manifest end).
    let short_wal_seg = test_wal_segment(20, wal_start, 2000, None); // Ends at 2000, but manifest expects 3000

    let short_plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: StorageTier::ColdStore,
            byte_count: 100 * 4096,
        }],
        wal_segment_copy_plan: vec![WalSegmentCopyTask {
            segment_descriptor: short_wal_seg,
            byte_count: 1000 * 1024,
            sequence_index: 0,
        }],
        validate_checksum_on_copy: false,
        resource_limits: test_resource_limits(),
        total_extent_bytes: 100 * 4096,
        total_wal_bytes: 1000 * 1024,
    };

    assert!(
        short_plan.validate().is_err(),
        "backup plan with WAL segments not covering full range should fail"
    );
}

#[test]
fn test_backup_artifact_metadata_consistency() {
    // If we corrupt one field (e.g., change total_extent_bytes), validation should fail.

    let manifest = test_manifest(3, 101, 300);
    let extent = test_extent(1, 1000, 100, ExtentState::PublishedCold);
    let wal_seg = test_wal_segment(1, 101, 300, None);

    let true_extent_bytes = 100 * 4096;
    let true_wal_bytes = 200 * 1024;

    let plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: StorageTier::ColdStore,
            byte_count: true_extent_bytes,
        }],
        wal_segment_copy_plan: vec![WalSegmentCopyTask {
            segment_descriptor: wal_seg,
            byte_count: true_wal_bytes,
            sequence_index: 0,
        }],
        validate_checksum_on_copy: false,
        resource_limits: test_resource_limits(),
        total_extent_bytes: true_extent_bytes,
        total_wal_bytes: true_wal_bytes,
    };

    // Plan should validate successfully with correct metadata.
    assert!(
        plan.validate().is_ok(),
        "backup plan with consistent metadata should pass validation"
    );

    // Test: Corrupt total_extent_bytes field.
    let corrupted_plan = BackupExecutionPlan {
        total_extent_bytes: true_extent_bytes + 1000, // Corrupted!
        ..plan.clone()
    };

    assert!(
        corrupted_plan.validate().is_err(),
        "backup plan with corrupted extent bytes should fail validation"
    );

    // Test: Corrupt total_wal_bytes field.
    let corrupted_plan = BackupExecutionPlan {
        total_wal_bytes: true_wal_bytes - 1000, // Corrupted!
        ..plan.clone()
    };

    assert!(
        corrupted_plan.validate().is_err(),
        "backup plan with corrupted WAL bytes should fail validation"
    );

    // Test: Compute total successfully via the plan method.
    assert_eq!(
        plan.total_extent_bytes, true_extent_bytes,
        "plan extent bytes should match true value"
    );
    assert_eq!(
        plan.total_wal_bytes, true_wal_bytes,
        "plan WAL bytes should match true value"
    );

    let total = plan.total_bytes().unwrap();
    assert_eq!(
        total,
        true_extent_bytes + true_wal_bytes,
        "plan total bytes should sum extent and WAL"
    );

    // Test: Corrupt extent count vs. plan length (use cloned plan to avoid move).
    let extent2 = test_extent(2, 1100, 50, ExtentState::PublishedCold);
    let mut extra_extent_plan = plan.clone();
    extra_extent_plan.extent_copy_plan.push(ExtentCopyTask {
        extent_descriptor: extent2,
        source_tier: StorageTier::ColdStore,
        byte_count: 50 * 4096,
    });
    extra_extent_plan.total_extent_bytes = true_extent_bytes; // Doesn't match!

    assert!(
        extra_extent_plan.validate().is_err(),
        "backup plan with extent count mismatch should fail validation"
    );
}

#[test]
fn test_backup_rejects_incomplete_wal() {
    let manifest = test_manifest(4, 101, 500);
    let extent = test_extent(1, 1000, 100, ExtentState::PublishedCold);

    let wal_seg1 = test_wal_segment(1, 101, 200, None);
    // Missing segments from 201 to 299
    let wal_seg2 = test_wal_segment(2, 300, 500, None); // WRONG: should chain from 200

    let bad_plan_gap = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: StorageTier::ColdStore,
            byte_count: 100 * 4096,
        }],
        wal_segment_copy_plan: vec![
            WalSegmentCopyTask {
                segment_descriptor: wal_seg1,
                byte_count: 100 * 1024,
                sequence_index: 0,
            },
            WalSegmentCopyTask {
                segment_descriptor: wal_seg2,
                byte_count: 200 * 1024,
                sequence_index: 1,
            },
        ],
        validate_checksum_on_copy: false,
        resource_limits: test_resource_limits(),
        total_extent_bytes: 100 * 4096,
        total_wal_bytes: 300 * 1024,
    };

    assert!(
        bad_plan_gap.validate().is_err(),
        "backup plan with WAL LSN gap should fail validation"
    );

    let late_start_seg = test_wal_segment(3, 150, 500, None); // Should start at 101, not 150

    let bad_plan_late_start = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: StorageTier::ColdStore,
            byte_count: 100 * 4096,
        }],
        wal_segment_copy_plan: vec![WalSegmentCopyTask {
            segment_descriptor: late_start_seg,
            byte_count: 350 * 1024,
            sequence_index: 0,
        }],
        validate_checksum_on_copy: false,
        resource_limits: test_resource_limits(),
        total_extent_bytes: 100 * 4096,
        total_wal_bytes: 350 * 1024,
    };

    assert!(
        bad_plan_late_start.validate().is_err(),
        "backup plan with incorrect WAL start LSN should fail validation"
    );

    let early_end_seg = test_wal_segment(4, 101, 400, None); // Should end at 500, not 400

    let bad_plan_early_end = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: StorageTier::ColdStore,
            byte_count: 100 * 4096,
        }],
        wal_segment_copy_plan: vec![WalSegmentCopyTask {
            segment_descriptor: early_end_seg,
            byte_count: 300 * 1024,
            sequence_index: 0,
        }],
        validate_checksum_on_copy: false,
        resource_limits: test_resource_limits(),
        total_extent_bytes: 100 * 4096,
        total_wal_bytes: 300 * 1024,
    };

    assert!(
        bad_plan_early_end.validate().is_err(),
        "backup plan with incorrect WAL end LSN should fail validation"
    );

    let wal_seg_chain1 = test_wal_segment(10, 101, 250, None);
    let wal_seg_chain2 = test_wal_segment(11, 251, 500, Some(249)); // Wrong: should be Some(250)

    let bad_plan_chain = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: StorageTier::ColdStore,
            byte_count: 100 * 4096,
        }],
        wal_segment_copy_plan: vec![
            WalSegmentCopyTask {
                segment_descriptor: wal_seg_chain1,
                byte_count: 150 * 1024,
                sequence_index: 0,
            },
            WalSegmentCopyTask {
                segment_descriptor: wal_seg_chain2,
                byte_count: 250 * 1024,
                sequence_index: 1,
            },
        ],
        validate_checksum_on_copy: false,
        resource_limits: test_resource_limits(),
        total_extent_bytes: 100 * 4096,
        total_wal_bytes: 400 * 1024,
    };

    assert!(
        bad_plan_chain.validate().is_err(),
        "backup plan with WAL segment chain breakage should fail validation"
    );

    let wal_seg_dup1 = test_wal_segment(20, 101, 250, None);
    let wal_seg_dup2 = test_wal_segment(20, 251, 500, Some(250)); // Same segment_id!

    let bad_plan_dup = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: StorageTier::ColdStore,
            byte_count: 100 * 4096,
        }],
        wal_segment_copy_plan: vec![
            WalSegmentCopyTask {
                segment_descriptor: wal_seg_dup1,
                byte_count: 150 * 1024,
                sequence_index: 0,
            },
            WalSegmentCopyTask {
                segment_descriptor: wal_seg_dup2,
                byte_count: 250 * 1024,
                sequence_index: 1,
            },
        ],
        validate_checksum_on_copy: false,
        resource_limits: test_resource_limits(),
        total_extent_bytes: 100 * 4096,
        total_wal_bytes: 400 * 1024,
    };

    assert!(
        bad_plan_dup.validate().is_err(),
        "backup plan with duplicate WAL segment IDs should fail validation"
    );

    let wal_seg_good1 = test_wal_segment(30, 101, 250, None);
    let wal_seg_good2 = test_wal_segment(31, 251, 500, Some(250));

    let good_plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: StorageTier::ColdStore,
            byte_count: 100 * 4096,
        }],
        wal_segment_copy_plan: vec![
            WalSegmentCopyTask {
                segment_descriptor: wal_seg_good1,
                byte_count: 150 * 1024,
                sequence_index: 0,
            },
            WalSegmentCopyTask {
                segment_descriptor: wal_seg_good2,
                byte_count: 250 * 1024,
                sequence_index: 1,
            },
        ],
        validate_checksum_on_copy: false,
        resource_limits: test_resource_limits(),
        total_extent_bytes: 100 * 4096,
        total_wal_bytes: 400 * 1024,
    };

    assert!(
        good_plan.validate().is_ok(),
        "backup plan with complete WAL coverage should pass validation"
    );
}

#[test]
fn file_backed_artifact_store_writes_manifest_snapshot_and_wal() {
    let temp = tempfile::TempDir::new().unwrap();
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();

    let snapshot_bytes = b"durable snapshot bytes for backup artifact";
    let wal1_bytes = b"durable wal segment one";
    let wal2_bytes = b"durable wal segment two";
    let manifest = test_manifest(55, 101, 300);
    let extent = test_extent(1, 1000, 1, ExtentState::PublishedCold);
    let wal_seg1 = test_wal_segment(10, 101, 200, None);
    let wal_seg2 = test_wal_segment(11, 201, 300, Some(200));

    let plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: StorageTier::ColdStore,
            byte_count: snapshot_bytes.len() as u64,
        }],
        wal_segment_copy_plan: vec![
            WalSegmentCopyTask {
                segment_descriptor: wal_seg1,
                byte_count: wal1_bytes.len() as u64,
                sequence_index: 0,
            },
            WalSegmentCopyTask {
                segment_descriptor: wal_seg2,
                byte_count: wal2_bytes.len() as u64,
                sequence_index: 1,
            },
        ],
        validate_checksum_on_copy: true,
        resource_limits: test_resource_limits(),
        total_extent_bytes: snapshot_bytes.len() as u64,
        total_wal_bytes: (wal1_bytes.len() + wal2_bytes.len()) as u64,
    };

    let report = store
        .write_execution_plan_artifact(
            &plan,
            snapshot_bytes,
            &[wal1_bytes.as_slice(), wal2_bytes.as_slice()],
        )
        .unwrap();

    assert!(report.manifest_path.exists());
    assert!(report.snapshot_path.exists());
    assert_eq!(report.wal_segment_paths.len(), 2);
    assert_eq!(
        report.source_checkpoint_lsn,
        manifest.snapshot.base_checkpoint_lsn
    );
    assert_eq!(
        report.wal_archive_evidence.total_bytes,
        (wal1_bytes.len() + wal2_bytes.len()) as u64
    );
    assert_ne!(
        report.wal_archive_evidence.archive_digest_sha256, [0; 32],
        "WAL archive evidence must carry an aggregate durable digest"
    );
    assert_eq!(
        report.compatibility_evidence.manifest_format_version,
        CURRENT_ARTIFACT_MANIFEST_FORMAT_VERSION
    );
    assert_eq!(report.compatibility_evidence.physical_plan_version, 1);
    assert_eq!(report.compatibility_evidence.storage_format_version, 1);
    assert_eq!(report.compatibility_evidence.wal_format_version, 1);
    assert!(
        report.compatibility_evidence.recorded_in_manifest,
        "current backup manifests must persist compatibility evidence"
    );

    let reopened = FileBackedBackupArtifactStore::open_existing(temp.path()).unwrap();
    let record = reopened
        .validate_artifact_directory(BackupId::new(55))
        .unwrap();
    assert_eq!(record.manifest, manifest);
    assert_eq!(
        record.manifest_format_version,
        CURRENT_ARTIFACT_MANIFEST_FORMAT_VERSION
    );
    assert_eq!(record.compatibility_evidence, report.compatibility_evidence);
    assert_eq!(
        record.artifact_set.cold_snapshot.artifact.byte_len,
        snapshot_bytes.len() as u64
    );
    assert_eq!(record.artifact_set.wal_segments.len(), 2);
    assert_eq!(
        record.wal_archive_evidence.archive_digest_sha256,
        report.wal_archive_evidence.archive_digest_sha256
    );
}

#[test]
fn file_backed_artifact_store_rejects_incompatible_v3_wal_format_evidence() {
    let temp = tempfile::TempDir::new().unwrap();
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();

    let snapshot_bytes = b"snapshot bytes with compatibility corruption";
    let wal_bytes = b"durable wal segment";
    let manifest = test_manifest(61, 101, 200);
    let extent = test_extent(1, 1000, 1, ExtentState::PublishedCold);
    let wal_seg = test_wal_segment(10, 101, 200, None);

    let plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: StorageTier::ColdStore,
            byte_count: snapshot_bytes.len() as u64,
        }],
        wal_segment_copy_plan: vec![WalSegmentCopyTask {
            segment_descriptor: wal_seg,
            byte_count: wal_bytes.len() as u64,
            sequence_index: 0,
        }],
        validate_checksum_on_copy: true,
        resource_limits: test_resource_limits(),
        total_extent_bytes: snapshot_bytes.len() as u64,
        total_wal_bytes: wal_bytes.len() as u64,
    };

    let report = store
        .write_execution_plan_artifact(&plan, snapshot_bytes, &[wal_bytes.as_slice()])
        .unwrap();
    rewrite_compatibility_wal_format(&report.manifest_path, 999);

    let err = store
        .validate_artifact_directory(BackupId::new(61))
        .unwrap_err();
    assert!(
        err.message().contains("WAL format"),
        "unexpected error: {err}"
    );
}

#[test]
fn file_backed_artifact_store_rejects_manifest_wal_segment_count_above_bound() {
    let temp = tempfile::TempDir::new().unwrap();
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();

    let snapshot_bytes = b"snapshot bytes with oversized manifest count";
    let wal_bytes = b"durable wal segment";
    let manifest = test_manifest(62, 101, 200);
    let extent = test_extent(1, 1000, 1, ExtentState::PublishedCold);
    let wal_seg = test_wal_segment(10, 101, 200, None);

    let plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: StorageTier::ColdStore,
            byte_count: snapshot_bytes.len() as u64,
        }],
        wal_segment_copy_plan: vec![WalSegmentCopyTask {
            segment_descriptor: wal_seg,
            byte_count: wal_bytes.len() as u64,
            sequence_index: 0,
        }],
        validate_checksum_on_copy: true,
        resource_limits: test_resource_limits(),
        total_extent_bytes: snapshot_bytes.len() as u64,
        total_wal_bytes: wal_bytes.len() as u64,
    };

    let report = store
        .write_execution_plan_artifact(&plan, snapshot_bytes, &[wal_bytes.as_slice()])
        .unwrap();
    rewrite_manifest_wal_segment_count(&report.manifest_path, 16_385);

    let err = store
        .validate_artifact_directory(BackupId::new(62))
        .unwrap_err();
    assert!(
        err.message().contains("bounded manifest segment limit"),
        "unexpected error: {err}"
    );
}

#[test]
fn file_backed_artifact_store_rejects_cold_snapshot_manifest_version_mismatch() {
    let temp = tempfile::TempDir::new().unwrap();
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();

    let snapshot_bytes = b"snapshot bytes with manifest version mismatch";
    let wal_bytes = b"durable wal segment";
    let manifest = test_manifest(63, 101, 200);
    let extent = test_extent(1, 1000, 1, ExtentState::PublishedCold);
    let wal_seg = test_wal_segment(10, 101, 200, None);

    let plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: StorageTier::ColdStore,
            byte_count: snapshot_bytes.len() as u64,
        }],
        wal_segment_copy_plan: vec![WalSegmentCopyTask {
            segment_descriptor: wal_seg,
            byte_count: wal_bytes.len() as u64,
            sequence_index: 0,
        }],
        validate_checksum_on_copy: true,
        resource_limits: test_resource_limits(),
        total_extent_bytes: snapshot_bytes.len() as u64,
        total_wal_bytes: wal_bytes.len() as u64,
    };

    let report = store
        .write_execution_plan_artifact(&plan, snapshot_bytes, &[wal_bytes.as_slice()])
        .unwrap();
    rewrite_cold_snapshot_manifest_version(&report.manifest_path, manifest.created_epoch + 1);

    let err = store
        .validate_artifact_directory(BackupId::new(63))
        .unwrap_err();
    assert!(
        err.message().contains("manifest version"),
        "unexpected error: {err}"
    );
}

#[test]
fn file_backed_artifact_store_reads_legacy_v2_manifest_without_compatibility_evidence() {
    let temp = tempfile::TempDir::new().unwrap();
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();

    let snapshot_bytes = b"legacy v2 snapshot bytes for backup artifact";
    let wal1_bytes = b"legacy v2 wal segment one";
    let wal2_bytes = b"legacy v2 wal segment two";
    let manifest = test_manifest(60, 101, 300);
    let extent = test_extent(1, 1000, 1, ExtentState::PublishedCold);
    let wal_seg1 = test_wal_segment(10, 101, 200, None);
    let wal_seg2 = test_wal_segment(11, 201, 300, Some(200));

    let plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: StorageTier::ColdStore,
            byte_count: snapshot_bytes.len() as u64,
        }],
        wal_segment_copy_plan: vec![
            WalSegmentCopyTask {
                segment_descriptor: wal_seg1,
                byte_count: wal1_bytes.len() as u64,
                sequence_index: 0,
            },
            WalSegmentCopyTask {
                segment_descriptor: wal_seg2,
                byte_count: wal2_bytes.len() as u64,
                sequence_index: 1,
            },
        ],
        validate_checksum_on_copy: true,
        resource_limits: test_resource_limits(),
        total_extent_bytes: snapshot_bytes.len() as u64,
        total_wal_bytes: (wal1_bytes.len() + wal2_bytes.len()) as u64,
    };

    let report = store
        .write_execution_plan_artifact(
            &plan,
            snapshot_bytes,
            &[wal1_bytes.as_slice(), wal2_bytes.as_slice()],
        )
        .unwrap();
    rewrite_manifest_to_v2_without_compatibility_evidence(&report.manifest_path);

    let record = store
        .validate_artifact_directory(BackupId::new(60))
        .unwrap();
    assert_eq!(
        record.manifest_format_version,
        LEGACY_V2_ARTIFACT_MANIFEST_FORMAT_VERSION
    );
    assert!(!record.compatibility_evidence.recorded_in_manifest);
    assert_eq!(
        record.compatibility_evidence.manifest_format_version,
        LEGACY_V2_ARTIFACT_MANIFEST_FORMAT_VERSION
    );
    assert_eq!(record.compatibility_evidence.storage_format_version, 1);
    assert_eq!(record.compatibility_evidence.wal_format_version, 1);
    assert_eq!(
        record.wal_archive_evidence.archive_digest_sha256,
        report.wal_archive_evidence.archive_digest_sha256
    );
}

#[test]
fn file_backed_artifact_store_reads_legacy_v1_manifest_with_reconstructed_wal_digest() {
    let temp = tempfile::TempDir::new().unwrap();
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();

    let snapshot_bytes = b"legacy snapshot bytes for backup artifact";
    let wal1_bytes = b"legacy wal segment one";
    let wal2_bytes = b"legacy wal segment two";
    let manifest = test_manifest(57, 101, 300);
    let extent = test_extent(1, 1000, 1, ExtentState::PublishedCold);
    let wal_seg1 = test_wal_segment(10, 101, 200, None);
    let wal_seg2 = test_wal_segment(11, 201, 300, Some(200));

    let plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: StorageTier::ColdStore,
            byte_count: snapshot_bytes.len() as u64,
        }],
        wal_segment_copy_plan: vec![
            WalSegmentCopyTask {
                segment_descriptor: wal_seg1,
                byte_count: wal1_bytes.len() as u64,
                sequence_index: 0,
            },
            WalSegmentCopyTask {
                segment_descriptor: wal_seg2,
                byte_count: wal2_bytes.len() as u64,
                sequence_index: 1,
            },
        ],
        validate_checksum_on_copy: true,
        resource_limits: test_resource_limits(),
        total_extent_bytes: snapshot_bytes.len() as u64,
        total_wal_bytes: (wal1_bytes.len() + wal2_bytes.len()) as u64,
    };

    let report = store
        .write_execution_plan_artifact(
            &plan,
            snapshot_bytes,
            &[wal1_bytes.as_slice(), wal2_bytes.as_slice()],
        )
        .unwrap();
    rewrite_manifest_to_v1_without_archive_digest(&report.manifest_path);

    let record = store
        .validate_artifact_directory(BackupId::new(57))
        .unwrap();
    assert_eq!(record.manifest_format_version, 1);
    assert!(!record.compatibility_evidence.recorded_in_manifest);
    assert_eq!(
        record.compatibility_evidence.manifest_format_version,
        LEGACY_V1_ARTIFACT_MANIFEST_FORMAT_VERSION
    );
    assert_eq!(
        record.wal_archive_evidence.archive_digest_sha256,
        report.wal_archive_evidence.archive_digest_sha256
    );
}

#[test]
fn file_backed_artifact_store_rejects_zero_wal_archive_evidence_digest() {
    let temp = tempfile::TempDir::new().unwrap();
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();

    let snapshot_bytes = b"snapshot bytes with evidence digest corruption";
    let wal_bytes = b"durable wal segment";
    let manifest = test_manifest(58, 101, 200);
    let extent = test_extent(1, 1000, 1, ExtentState::PublishedCold);
    let wal_seg = test_wal_segment(10, 101, 200, None);

    let plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: StorageTier::ColdStore,
            byte_count: snapshot_bytes.len() as u64,
        }],
        wal_segment_copy_plan: vec![WalSegmentCopyTask {
            segment_descriptor: wal_seg,
            byte_count: wal_bytes.len() as u64,
            sequence_index: 0,
        }],
        validate_checksum_on_copy: true,
        resource_limits: test_resource_limits(),
        total_extent_bytes: snapshot_bytes.len() as u64,
        total_wal_bytes: wal_bytes.len() as u64,
    };

    let report = store
        .write_execution_plan_artifact(&plan, snapshot_bytes, &[wal_bytes.as_slice()])
        .unwrap();
    zero_manifest_archive_digest(&report.manifest_path);

    let err = store
        .validate_artifact_directory(BackupId::new(58))
        .unwrap_err();
    assert!(
        err.message().contains("evidence digest"),
        "unexpected error: {err}"
    );
}

#[test]
fn file_backed_artifact_store_rejects_cold_snapshot_crc_mismatch() {
    let temp = tempfile::TempDir::new().unwrap();
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();

    let snapshot_bytes = b"snapshot bytes with cold snapshot crc mismatch";
    let wal_bytes = b"durable wal segment";
    let manifest = test_manifest(59, 101, 200);
    let extent = test_extent(1, 1000, 1, ExtentState::PublishedCold);
    let wal_seg = test_wal_segment(10, 101, 200, None);

    let plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: StorageTier::ColdStore,
            byte_count: snapshot_bytes.len() as u64,
        }],
        wal_segment_copy_plan: vec![WalSegmentCopyTask {
            segment_descriptor: wal_seg,
            byte_count: wal_bytes.len() as u64,
            sequence_index: 0,
        }],
        validate_checksum_on_copy: true,
        resource_limits: test_resource_limits(),
        total_extent_bytes: snapshot_bytes.len() as u64,
        total_wal_bytes: wal_bytes.len() as u64,
    };

    let report = store
        .write_execution_plan_artifact(&plan, snapshot_bytes, &[wal_bytes.as_slice()])
        .unwrap();
    rewrite_cold_snapshot_manifest_crc(&report.manifest_path, manifest.manifest_crc + 1);

    let err = store
        .validate_artifact_directory(BackupId::new(59))
        .unwrap_err();
    assert!(
        err.message().contains("manifest CRC"),
        "unexpected error: {err}"
    );
}

#[test]
fn file_backed_artifact_store_rejects_corrupted_snapshot() {
    let temp = tempfile::TempDir::new().unwrap();
    let store = FileBackedBackupArtifactStore::open(temp.path()).unwrap();

    let snapshot_bytes = b"snapshot before corruption";
    let wal_bytes = b"durable wal segment";
    let manifest = test_manifest(56, 101, 200);
    let extent = test_extent(1, 1000, 1, ExtentState::PublishedCold);
    let wal_seg = test_wal_segment(10, 101, 200, None);

    let plan = BackupExecutionPlan {
        manifest,
        extent_copy_plan: vec![ExtentCopyTask {
            extent_descriptor: extent,
            source_tier: StorageTier::ColdStore,
            byte_count: snapshot_bytes.len() as u64,
        }],
        wal_segment_copy_plan: vec![WalSegmentCopyTask {
            segment_descriptor: wal_seg,
            byte_count: wal_bytes.len() as u64,
            sequence_index: 0,
        }],
        validate_checksum_on_copy: true,
        resource_limits: test_resource_limits(),
        total_extent_bytes: snapshot_bytes.len() as u64,
        total_wal_bytes: wal_bytes.len() as u64,
    };

    let report = store
        .write_execution_plan_artifact(&plan, snapshot_bytes, &[wal_bytes.as_slice()])
        .unwrap();
    let mut corrupted_snapshot = snapshot_bytes.to_vec();
    corrupted_snapshot[0] ^= 0xFF;
    std::fs::write(&report.snapshot_path, corrupted_snapshot).unwrap();

    let err = store
        .validate_artifact_directory(BackupId::new(56))
        .unwrap_err();
    assert!(
        err.message()
            .contains("snapshot artifact checksum mismatch"),
        "unexpected error: {err}"
    );
}
