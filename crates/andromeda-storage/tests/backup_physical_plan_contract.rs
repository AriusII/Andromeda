//! F5 Backup Physical Plan Contract Tests
//!
//! Comprehensive test suite validating:
//! - BackupPhysicalPlan creation and validation
//! - Segment scheduling (HotStore first, ColdStore second)
//! - I/O budget respect (throughput ceiling enforcement)
//! - Checkpoint creation and crash recovery
//! - Checksum validation on resumed backup
//! - WAL archive validation
//! - PITR window computation
//! - Error cases (invalid IDs, checksum mismatches, WAL gaps)
//!
//! Total: 21 tests

#[cfg(test)]
mod tests {
    use andromeda_observe::TraceId;
    use andromeda_storage::backup::*;

    // ============ Test Helpers ============

    fn create_basic_physical_plan() -> BackupPhysicalPlan {
        BackupPhysicalPlan::new(
            BackupId::new(1),
            Lsn::new(1000),
            vec![SegmentPlan::ColdStoreScan {
                cold_segment_id: 1,
                page_count: 100,
            }],
            100,
            TraceId::new(1),
            100,
        )
    }

    fn create_mixed_plan() -> BackupPhysicalPlan {
        BackupPhysicalPlan::new(
            BackupId::new(2),
            Lsn::new(2000),
            vec![
                SegmentPlan::HotStoreScan { extent_range: 0..5 },
                SegmentPlan::ColdStoreScan {
                    cold_segment_id: 10,
                    page_count: 500,
                },
            ],
            1280, // 5 extents * 256 pages + 500 cold pages
            TraceId::new(2),
            200,
        )
    }

    // ============ Test Set 1: BackupPhysicalPlan Creation (2 tests) ============

    #[test]
    fn test_backup_physical_plan_creation_and_validation() {
        let plan = create_basic_physical_plan();
        assert_eq!(plan.backup_id.get(), 1);
        assert_eq!(plan.catalog_snapshot_lsn, Lsn::new(1000));
        assert_eq!(plan.total_pages, 100);
        assert!(!plan.segments_to_scan.is_empty());
        assert!(plan.validate().is_ok());
    }

    #[test]
    fn test_backup_physical_plan_rejects_invalid_backup_id() {
        let plan = BackupPhysicalPlan::new(
            BackupId::new(0), // Invalid: zero backup ID
            Lsn::new(1000),
            vec![SegmentPlan::ColdStoreScan {
                cold_segment_id: 1,
                page_count: 100,
            }],
            100,
            TraceId::new(1),
            100,
        );
        assert!(plan.validate().is_err());
    }

    // ============ Test Set 2: Segment Scheduling - Order Enforcement (3 tests) ============

    #[test]
    fn test_hot_store_scheduled_before_cold_store() {
        let plan = create_mixed_plan();
        assert!(plan.validate().is_ok());

        // Verify HotStore segment comes first
        if let Some(SegmentPlan::HotStoreScan { .. }) = plan.segments_to_scan.first() {
            // Good
        } else {
            panic!("Expected HotStore as first segment");
        }
    }

    #[test]
    fn test_cold_store_only_plan_valid() {
        let plan = BackupPhysicalPlan::new(
            BackupId::new(1),
            Lsn::new(1000),
            vec![
                SegmentPlan::ColdStoreScan {
                    cold_segment_id: 1,
                    page_count: 100,
                },
                SegmentPlan::ColdStoreScan {
                    cold_segment_id: 2,
                    page_count: 200,
                },
            ],
            300,
            TraceId::new(1),
            100,
        );
        assert!(plan.validate().is_ok());
    }

    #[test]
    fn test_wrong_segment_order_rejected() {
        let plan = BackupPhysicalPlan::new(
            BackupId::new(1),
            Lsn::new(1000),
            vec![
                SegmentPlan::ColdStoreScan {
                    cold_segment_id: 1,
                    page_count: 100,
                },
                SegmentPlan::HotStoreScan { extent_range: 0..5 }, // ColdStore before HotStore — INVALID
            ],
            1300,
            TraceId::new(1),
            100,
        );
        assert!(plan.validate().is_err());
    }

    // ============ Test Set 3: I/O Budget Respect (3 tests) ============

    #[test]
    fn test_io_scheduler_respects_nvme_budget() {
        let scheduler = BackupIOScheduler::new(4096, 500, 100); // 500 MB/s NVMe, 100 MB/s HDD
        let plan = create_basic_physical_plan();
        let schedule = scheduler.schedule_page_scan(&plan).unwrap();

        assert!(schedule.respects_budget(500, 100).is_ok());
    }

    #[test]
    fn test_io_scheduler_rejects_excessive_throughput() {
        let scheduler = BackupIOScheduler::new(4096, 1000, 500); // High throughput
        let plan = create_basic_physical_plan();
        let schedule = scheduler.schedule_page_scan(&plan).unwrap();

        // Requesting very tight budget should fail
        assert!(schedule.respects_budget(10, 5).is_err());
    }

    #[test]
    fn test_io_schedule_total_bytes_non_zero() {
        let scheduler = BackupIOScheduler::new(4096, 1000, 100);
        let plan = create_mixed_plan();
        let schedule = scheduler.schedule_page_scan(&plan).unwrap();

        assert!(schedule.total_bytes > 0);
        assert!(schedule.peak_throughput_mbps > 0);
    }

    // ============ Test Set 4: Checkpoint Creation and Recovery (4 tests) ============

    #[test]
    fn test_backup_checkpoint_persist_valid() {
        let checkpoint = BackupCheckpoint {
            phase: BackupPhase::HotStoreScan,
            last_completed_page: 5000,
            checksum: 0xCAFEBABE,
            checkpoint_epoch: 100,
        };
        assert!(checkpoint.validate().is_ok());
    }

    #[test]
    fn test_backup_checkpoint_rejects_zero_epoch() {
        let checkpoint = BackupCheckpoint {
            phase: BackupPhase::HotStoreScan,
            last_completed_page: 5000,
            checksum: 0xDEADBEEF,
            checkpoint_epoch: 0, // Invalid
        };
        assert!(checkpoint.validate().is_err());
    }

    #[test]
    fn test_checkpoint_manager_persists_and_validates() {
        let manager = BackupCheckpointManager::new(BackupId::new(1), "/tmp/backup".to_string());

        let checkpoint = BackupCheckpoint {
            phase: BackupPhase::ColdStoreScan,
            last_completed_page: 10000,
            checksum: 0xABCDEF00,
            checkpoint_epoch: 200,
        };

        assert!(manager.persist_backup_checkpoint(&checkpoint).is_ok());
    }

    #[test]
    fn test_checkpoint_recovery_mismatch_fails() {
        let manager = BackupCheckpointManager::new(BackupId::new(1), "/tmp/backup".to_string());

        // Recovery with mismatched backup ID should fail
        let recovery = manager.recover_backup_from_checkpoint(BackupId::new(999));
        assert!(recovery.is_err());
    }

    // ============ Test Set 5: Checksum Validation on Resumed Backup (2 tests) ============

    #[test]
    fn test_checksum_validation_same_data() {
        let manager = BackupCheckpointManager::new(BackupId::new(1), "/tmp/backup".to_string());

        let data = b"test backup data for checksum";
        let checksum = manager.compute_data_checksum(data);

        assert!(
            manager
                .validate_checksum_after_resumption(checksum, data)
                .is_ok()
        );
    }

    #[test]
    fn test_checksum_validation_corrupt_data() {
        let manager = BackupCheckpointManager::new(BackupId::new(1), "/tmp/backup".to_string());

        let original_data = b"original data";
        let corrupted_data = b"corrupted data";

        let checksum = manager.compute_data_checksum(original_data);

        assert!(
            manager
                .validate_checksum_after_resumption(checksum, corrupted_data)
                .is_err()
        );
    }

    // ============ Test Set 6: WAL Archive Validation (2 tests) ============

    #[test]
    fn test_wal_archive_validation_sufficient_coverage() {
        let result = WalArchiveIntegration::validate_wal_archive(
            Lsn::new(1000), // catalog snapshot
            Lsn::new(900),  // wal start
            Lsn::new(2000), // wal end (covers snapshot)
            5,              // segment count
        );

        assert!(result.is_ok());
        let val = result.unwrap();
        assert!(val.is_valid);
        assert_eq!(val.segment_count, 5);
    }

    #[test]
    fn test_wal_archive_validation_insufficient_coverage() {
        let result = WalArchiveIntegration::validate_wal_archive(
            Lsn::new(2000), // catalog snapshot at 2000
            Lsn::new(900),  // wal start
            Lsn::new(1500), // wal end (does NOT cover snapshot)
            5,
        );

        assert!(result.is_err());
    }

    // ============ Test Set 7: PITR Window Computation (2 tests) ============

    #[test]
    fn test_pitr_window_computed_correctly() {
        let snapshot = ColdSnapshotBoundary {
            snapshot_id: 1,
            snapshot_descriptor_hash: [0xAA; 32],
            base_checkpoint_lsn: Lsn::new(1000),
            required_wal_start_lsn: Lsn::new(1000),
        };

        let manifest = BackupManifest {
            backup_id: BackupId::new(1),
            database_id: 42,
            created_epoch: 100,
            snapshot,
            wal_archive: WalArchiveRange::new(Lsn::new(1000), Lsn::new(2000)),
            manifest_crc: 0xDEADBEEF,
        };

        let (earliest, latest) = WalArchiveIntegration::compute_pitr_window(&manifest).unwrap();

        assert_eq!(earliest, Lsn::new(1000));
        assert_eq!(latest, Lsn::new(2000));
    }

    #[test]
    fn test_pitr_target_validation_in_range() {
        let snapshot = ColdSnapshotBoundary {
            snapshot_id: 1,
            snapshot_descriptor_hash: [0xBB; 32],
            base_checkpoint_lsn: Lsn::new(1000),
            required_wal_start_lsn: Lsn::new(1000),
        };

        let manifest = BackupManifest {
            backup_id: BackupId::new(1),
            database_id: 42,
            created_epoch: 100,
            snapshot,
            wal_archive: WalArchiveRange::new(Lsn::new(1000), Lsn::new(2000)),
            manifest_crc: 0xDEADBEEF,
        };

        // Valid targets
        assert!(WalArchiveIntegration::validate_pitr_target(&manifest, Lsn::new(1000)).is_ok());
        assert!(WalArchiveIntegration::validate_pitr_target(&manifest, Lsn::new(1500)).is_ok());
        assert!(WalArchiveIntegration::validate_pitr_target(&manifest, Lsn::new(2000)).is_ok());
    }

    // ============ Test Set 8: Error Cases (3 tests) ============

    #[test]
    fn test_backup_id_not_found_error() {
        let plan = BackupPhysicalPlan::new(
            BackupId::new(0),
            Lsn::new(1000),
            vec![SegmentPlan::ColdStoreScan {
                cold_segment_id: 1,
                page_count: 100,
            }],
            100,
            TraceId::new(1),
            100,
        );

        // Should reject zero backup ID
        assert!(plan.validate().is_err());
    }

    #[test]
    fn test_pitr_target_before_earliest() {
        let snapshot = ColdSnapshotBoundary {
            snapshot_id: 1,
            snapshot_descriptor_hash: [0xCC; 32],
            base_checkpoint_lsn: Lsn::new(1000),
            required_wal_start_lsn: Lsn::new(900),
        };

        let manifest = BackupManifest {
            backup_id: BackupId::new(1),
            database_id: 42,
            created_epoch: 100,
            snapshot,
            wal_archive: WalArchiveRange::new(Lsn::new(900), Lsn::new(2000)),
            manifest_crc: 0xDEADBEEF,
        };

        // Target before earliest should fail
        assert!(WalArchiveIntegration::validate_pitr_target(&manifest, Lsn::new(500)).is_err());
    }

    #[test]
    fn test_pitr_target_after_latest() {
        let snapshot = ColdSnapshotBoundary {
            snapshot_id: 1,
            snapshot_descriptor_hash: [0xDD; 32],
            base_checkpoint_lsn: Lsn::new(1000),
            required_wal_start_lsn: Lsn::new(900),
        };

        let manifest = BackupManifest {
            backup_id: BackupId::new(1),
            database_id: 42,
            created_epoch: 100,
            snapshot,
            wal_archive: WalArchiveRange::new(Lsn::new(900), Lsn::new(2000)),
            manifest_crc: 0xDEADBEEF,
        };

        // Target after latest should fail
        assert!(WalArchiveIntegration::validate_pitr_target(&manifest, Lsn::new(5000)).is_err());
    }
}
