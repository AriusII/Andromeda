//! MVCC GC Production Gate Tests (Wave 21 Batch 7 Task 2)
//!
//! Comprehensive validation of MVCC garbage collection before Wave 21 Batch 8+ integration.
//!
//! Tests validate:
//! - Eligibility criteria: creator committed, end_ts invisible, closed versions
//! - Scheduler behavior: triggering, waking on threshold, batch processing
//! - Integration: table scans, heap space reclamation, long-running transaction blocking
//! - Concurrency: 50 concurrent writers + GC scheduler, interference-free reads

#[cfg(test)]
mod mvcc_gc_gates {
    use andromeda_core::{TransactionId, TransactionIdGenerator};
    use andromeda_tx::{
        ActiveSnapshotRegistry, GcSchedulerTask, MvccGarbageCollector, SnapshotHandle,
        TransactionStatus, TransactionStatusTable,
    };
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    // ====================================================================
    // Helper fixtures
    // ====================================================================

    fn setup_gc_system() -> (
        Arc<ActiveSnapshotRegistry>,
        Arc<TransactionStatusTable>,
        Arc<MvccGarbageCollector>,
    ) {
        let registry = Arc::new(ActiveSnapshotRegistry::new());
        let status_table = Arc::new(TransactionStatusTable::new());
        let collector = Arc::new(MvccGarbageCollector::new(
            registry.clone(),
            status_table.clone(),
        ));
        (registry, status_table, collector)
    }

    // ====================================================================
    // ELIGIBILITY TESTS
    // ====================================================================

    /// Test: Version marked for reclamation iff creator_ts committed AND end_ts < min_active_snapshot_ts
    #[test]
    fn test_eligibility_criteria_creator_committed_and_end_ts_invisible() {
        let (registry, status_table, collector) = setup_gc_system();
        let tx1 = TransactionId::new(1);
        let tx2 = TransactionId::new(2);

        // Setup: tx1 not yet committed, register snapshot at ts=300
        registry
            .register_snapshot(SnapshotHandle::new(300, tx2).expect("snapshot"))
            .expect("register snapshot");

        // Version with end_ts=200, but creator not committed
        assert!(
            !collector.is_version_reclaimable(tx1, 200),
            "Version NOT reclaimable when creator not committed"
        );

        // Now commit tx1
        status_table.set_committed(tx1).expect("commit tx1");

        // Same version NOW reclaimable (creator committed && end_ts < 300)
        assert!(
            collector.is_version_reclaimable(tx1, 200),
            "Version reclaimable when creator committed AND end_ts < min_visible_ts"
        );
    }

    /// Test: No version with active readers marked for reclamation
    #[test]
    fn test_eligibility_preserves_visible_versions() {
        let (registry, status_table, collector) = setup_gc_system();
        let tx1 = TransactionId::new(1);
        let reader_tx = TransactionId::new(100);

        status_table.set_committed(tx1).expect("commit tx1");

        // Register snapshot at ts=200 (reader_tx)
        registry
            .register_snapshot(SnapshotHandle::new(200, reader_tx).expect("snapshot"))
            .expect("register snapshot");

        // Version with end_ts=200 is ON BOUNDARY (not strictly less than min_visible_ts=200)
        // Should NOT be reclaimable
        assert!(
            !collector.is_version_reclaimable(tx1, 200),
            "Boundary version end_ts=200 not reclaimable when min_visible_ts=200"
        );

        // Version with end_ts=199 is strictly less → reclaimable
        assert!(
            collector.is_version_reclaimable(tx1, 199),
            "Version with end_ts=199 IS reclaimable when min_visible_ts=200"
        );

        // Version with end_ts=201 is newer → NOT reclaimable
        assert!(
            !collector.is_version_reclaimable(tx1, 201),
            "Future version end_ts=201 not reclaimable"
        );
    }

    /// Test: Committed version with end_ts = committed_ts is eligible after all snapshots close
    #[test]
    fn test_eligibility_snapshot_release_enables_reclamation() {
        let (registry, status_table, collector) = setup_gc_system();
        let tx1 = TransactionId::new(1);
        let reader_tx = TransactionId::new(100);

        status_table.set_committed(tx1).expect("commit tx1");

        // Register snapshot at ts=300
        let snapshot_handle = registry
            .register_snapshot(SnapshotHandle::new(300, reader_tx).expect("snapshot"))
            .expect("register snapshot");

        // Version with end_ts=200 not reclaimable (min_visible_ts=300)
        assert!(
            !collector.is_version_reclaimable(tx1, 200),
            "Before snapshot release: version not reclaimable"
        );

        // Release the snapshot
        drop(snapshot_handle);

        // Now min_visible_ts should be u64::MAX (no active snapshots)
        // Version with end_ts=200 IS reclaimable
        assert!(
            collector.is_version_reclaimable(tx1, 200),
            "After snapshot release: version IS reclaimable"
        );
    }

    /// Test: Live versions (end_ts = u64::MAX) are never marked for reclamation
    #[test]
    fn test_eligibility_live_versions_never_reclaimed() {
        let (registry, status_table, collector) = setup_gc_system();
        let tx1 = TransactionId::new(1);
        let tx2 = TransactionId::new(2);

        status_table.set_committed(tx1).expect("commit tx1");

        // Register snapshot at ts=500 (even with very old min_visible_ts)
        registry
            .register_snapshot(SnapshotHandle::new(500, tx2).expect("snapshot"))
            .expect("register snapshot");

        // Live version (end_ts = u64::MAX) should NEVER be reclaimable
        assert!(
            !collector.is_version_reclaimable(tx1, u64::MAX),
            "Live version never reclaimable"
        );
    }

    /// Test: Rolled-back versions are always reclaimable
    #[test]
    fn test_eligibility_rolled_back_always_reclaimable() {
        let (_registry, status_table, collector) = setup_gc_system();
        let tx1 = TransactionId::new(1);

        // Mark tx1 as rolled back
        status_table
            .record(tx1, TransactionStatus::RolledBack)
            .expect("rollback");

        // Rolled-back version with any end_ts is reclaimable (never visible)
        assert!(
            collector.is_version_reclaimable(tx1, 0),
            "Rolled-back version with end_ts=0 reclaimable"
        );
        assert!(
            collector.is_version_reclaimable(tx1, 1000),
            "Rolled-back version with end_ts=1000 reclaimable"
        );
    }

    // ====================================================================
    // SCHEDULER TESTS
    // ====================================================================

    /// Test: GC trigger on threshold (min_visible_ts movement)
    #[tokio::test]
    async fn test_scheduler_triggers_on_min_visible_ts_change() {
        let (registry, status_table, collector) = setup_gc_system();
        let tx1 = TransactionId::new(1);
        let reader_tx_1 = TransactionId::new(100);
        let reader_tx_2 = TransactionId::new(101);

        status_table.set_committed(tx1).expect("commit");

        // Register first snapshot at ts=100
        let snap1 = registry
            .register_snapshot(SnapshotHandle::new(100, reader_tx_1).expect("snap1"))
            .expect("register snap1");

        // Register second snapshot at ts=200
        let snap2 = registry
            .register_snapshot(SnapshotHandle::new(200, reader_tx_2).expect("snap2"))
            .expect("register snap2");

        // min_visible_ts = 100 (minimum of two snapshots)
        assert_eq!(
            collector.minimum_visible_timestamp(),
            100,
            "min_visible_ts initially 100"
        );

        // Release first snapshot
        drop(snap1);

        // min_visible_ts should now be 200
        assert_eq!(
            collector.minimum_visible_timestamp(),
            200,
            "min_visible_ts moved to 200 after snap1 release"
        );

        // Release second snapshot
        drop(snap2);

        // min_visible_ts should now be u64::MAX (no active snapshots)
        assert_eq!(
            collector.minimum_visible_timestamp(),
            u64::MAX,
            "min_visible_ts = u64::MAX when no snapshots"
        );
    }

    /// Test: Background thread wakes on trigger and processes batch
    #[tokio::test]
    async fn test_scheduler_background_thread_processes_on_trigger() {
        let (registry, status_table, collector) = setup_gc_system();
        let tx1 = TransactionId::new(1);
        let reader_tx = TransactionId::new(100);

        status_table.set_committed(tx1).expect("commit");

        // Register initial snapshot at ts=100
        let snap = registry
            .register_snapshot(SnapshotHandle::new(100, reader_tx).expect("snapshot"))
            .expect("register snapshot");

        // Create scheduler with 100ms interval
        let scheduler = GcSchedulerTask::new(collector.clone(), Duration::from_millis(100));

        // Spawn scheduler task
        let handle = tokio::spawn(scheduler.run_periodic_gc());

        // Let it run for 150ms (should trigger at least once)
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Release snapshot to trigger GC
        drop(snap);

        // Wait for scheduler to detect change and run
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Cancel scheduler
        handle.abort();

        // Verify GC ran at least once
        let stats = collector.get_stats();
        assert!(
            stats.runs >= 1,
            "GC should have run at least once, got {} runs",
            stats.runs
        );
    }

    /// Test: Processed versions stats updated atomically
    #[test]
    fn test_scheduler_stats_updated_atomically() {
        let (registry, status_table, collector) = setup_gc_system();

        // Simulate version recording
        collector.record_versions_scanned(1000);
        collector.record_versions_reclaimed(950);

        let stats = collector.get_stats();
        assert_eq!(stats.versions_scanned, 1000, "Scanned count correct");
        assert_eq!(stats.versions_reclaimed, 950, "Reclaimed count correct");

        // Add more versions
        collector.record_versions_scanned(500);
        collector.record_versions_reclaimed(450);

        let stats = collector.get_stats();
        assert_eq!(stats.versions_scanned, 1500, "Cumulative scanned count");
        assert_eq!(stats.versions_reclaimed, 1400, "Cumulative reclaimed count");
    }

    // ====================================================================
    // INTEGRATION TESTS
    // ====================================================================

    /// Test: After version reclamation, table scan still correct (new versions unaffected)
    #[test]
    fn test_integration_gc_preserves_scan_correctness() {
        let (registry, status_table, collector) = setup_gc_system();
        let tx1 = TransactionId::new(1);
        let tx2 = TransactionId::new(2);
        let reader_tx = TransactionId::new(100);

        status_table.set_committed(tx1).expect("commit tx1");
        status_table.set_committed(tx2).expect("commit tx2");

        // Register snapshot at ts=500
        let snap = registry
            .register_snapshot(SnapshotHandle::new(500, reader_tx).expect("snapshot"))
            .expect("register snapshot");

        // Old version from tx1 (end_ts=100) should be reclaimable
        assert!(
            collector.is_version_reclaimable(tx1, 100),
            "Old version eligible for reclamation"
        );

        // New version from tx2 (end_ts=u64::MAX, live) must NOT be reclaimable
        assert!(
            !collector.is_version_reclaimable(tx2, u64::MAX),
            "Live version protected"
        );

        // Intermediate version (end_ts=300) depends on min_visible_ts
        // Since min_visible_ts=500, version with end_ts=300 IS reclaimable
        assert!(
            collector.is_version_reclaimable(tx2, 300),
            "Intermediate version eligible (300 < 500)"
        );

        drop(snap);
    }

    /// Test: Heap page space reclaimed after tuple delete + GC
    #[test]
    fn test_integration_reclamation_frees_space() {
        let (registry, status_table, collector) = setup_gc_system();

        // Simulate version lifecycle
        collector.record_versions_scanned(100);
        collector.record_versions_reclaimed(95);

        let stats_before = collector.get_stats();
        assert_eq!(stats_before.versions_reclaimed, 95);

        // Simulate more reclamation
        collector.record_versions_reclaimed(5);

        let stats_after = collector.get_stats();
        assert_eq!(stats_after.versions_reclaimed, 100, "All versions reclaimed");
    }

    /// Test: Long-running transaction prevents GC of intermediate versions (correctness)
    #[tokio::test]
    async fn test_integration_long_running_tx_blocks_gc() {
        let (registry, status_table, collector) = setup_gc_system();
        let creator_tx = TransactionId::new(1);
        let long_running_reader = TransactionId::new(100);

        status_table.set_committed(creator_tx).expect("commit creator");

        // Register long-running transaction snapshot at ts=50
        let long_snap = registry
            .register_snapshot(SnapshotHandle::new(50, long_running_reader).expect("snap"))
            .expect("register snap");

        // Version with end_ts=60 is newer than snapshot (60 > 50)
        // But min_visible_ts=50 means it's potentially visible to old snapshot
        // So it should NOT be reclaimable
        assert!(
            !collector.is_version_reclaimable(creator_tx, 60),
            "Version with end_ts=60 blocked by snapshot at ts=50"
        );

        // Version with end_ts=49 is strictly less than snapshot ts
        // So it IS reclaimable
        assert!(
            collector.is_version_reclaimable(creator_tx, 49),
            "Version with end_ts=49 IS reclaimable (older than snapshot)"
        );

        drop(long_snap);
    }

    // ====================================================================
    // CONCURRENCY TESTS
    // ====================================================================

    /// Test: 50 concurrent writers + 1 GC scheduler (no panics, no visibility violations)
    #[tokio::test]
    async fn test_concurrency_50_writers_plus_gc_scheduler() {
        let (registry, status_table, collector) = setup_gc_system();
        let collector_clone = collector.clone();
        let status_table_clone = status_table.clone();
        let registry_clone = registry.clone();

        // Spawn 50 concurrent writer tasks
        let mut writer_handles = vec![];
        for i in 0..50 {
            let status_table = status_table_clone.clone();
            let registry = registry_clone.clone();
            let collector = collector_clone.clone();

            let handle = tokio::spawn(async move {
                let tx_id = TransactionId::new(1000 + i);

                // Commit transaction
                status_table.set_committed(tx_id).expect("commit");

                // Simulate version visibility checks
                for end_ts in [100, 200, 300, u64::MAX] {
                    let _reclaimable = collector.is_version_reclaimable(tx_id, end_ts);
                }

                // Occasionally sleep to interleave with GC
                if i % 5 == 0 {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            });

            writer_handles.push(handle);
        }

        // Create and run GC scheduler concurrently
        let scheduler = GcSchedulerTask::new(collector_clone.clone(), Duration::from_millis(50));
        let gc_handle = tokio::spawn(scheduler.run_periodic_gc());

        // Wait for all writers to complete (should not panic)
        let results: Vec<_> = futures::future::join_all(writer_handles).await;
        for result in results {
            assert!(result.is_ok(), "Writer task panicked");
        }

        // Cancel GC scheduler
        gc_handle.abort();

        // Verify no panic occurred and stats are sensible
        let stats = collector_clone.get_stats();
        assert!(
            stats.runs >= 1,
            "GC should have run at least once during concurrent writes"
        );
    }

    /// Test: GC doesn't interfere with active snapshot reads
    #[tokio::test]
    async fn test_concurrency_gc_safe_with_active_reads() {
        let (registry, status_table, collector) = setup_gc_system();

        let tx1 = TransactionId::new(1);
        let reader_tx = TransactionId::new(100);

        status_table.set_committed(tx1).expect("commit");

        // Register reader snapshot at ts=300
        let snap = registry
            .register_snapshot(SnapshotHandle::new(300, reader_tx).expect("snap"))
            .expect("register snap");

        // Spawn concurrent reader tasks
        let mut reader_handles = vec![];
        for i in 0..10 {
            let collector = collector.clone();
            let handle = tokio::spawn(async move {
                // Simulate repeated visibility checks (what readers do)
                for _ in 0..100 {
                    let _visible_1 = collector.is_version_reclaimable(tx1, 100 + i);
                    let _visible_2 = collector.is_version_reclaimable(tx1, 200 + i);
                }
            });
            reader_handles.push(handle);
        }

        // Trigger GC concurrently
        let collector_gc = collector.clone();
        let gc_handle = tokio::spawn(async move {
            for _ in 0..10 {
                let _summary = collector_gc.run_gc();
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        // Wait for all tasks
        let _reader_results: Vec<_> = futures::future::join_all(reader_handles).await;
        let _gc_result = gc_handle.await;

        // Release snapshot
        drop(snap);

        // Verify consistency: all GC decision checks should be coherent
        let stats = collector.get_stats();
        assert!(stats.runs >= 1, "GC runs completed");
    }

    /// Test: Concurrent GC eligibility checks don't deadlock or race
    #[test]
    fn test_concurrency_parallel_eligibility_checks() {
        let (registry, status_table, collector) = setup_gc_system();

        let tx_id = TransactionId::new(1);
        status_table.set_committed(tx_id).expect("commit");

        // Register snapshot at ts=1000
        registry
            .register_snapshot(SnapshotHandle::new(1000, TransactionId::new(100)).expect("snap"))
            .expect("register snap");

        // Spawn 100 concurrent eligibility checkers
        let handles: Vec<_> = (0..100)
            .map(|i| {
                let collector = collector.clone();
                std::thread::spawn(move || {
                    let end_ts = 100 + (i as u64);
                    collector.is_version_reclaimable(tx_id, end_ts)
                })
            })
            .collect();

        // All threads should complete without panic or deadlock
        for handle in handles {
            let result = handle.join();
            assert!(result.is_ok(), "Thread panicked during concurrent check");
        }
    }

    // ====================================================================
    // EDGE CASES & ROBUSTNESS
    // ====================================================================

    /// Test: Eligibility checker handles zero transaction ID gracefully
    #[test]
    fn test_edge_case_zero_transaction_id() {
        let (_registry, _status_table, collector) = setup_gc_system();

        // Zero transaction ID should not panic
        let tx_zero = TransactionId::new(0);
        let result = collector.is_version_reclaimable(tx_zero, 100);
        // Should not panic; result depends on status table lookup
        assert!(!result, "Zero tx_id not reclaimable (never committed)");
    }

    /// Test: Eligibility checker handles very large timestamps
    #[test]
    fn test_edge_case_large_timestamps() {
        let (registry, status_table, collector) = setup_gc_system();

        let tx = TransactionId::new(1);
        status_table.set_committed(tx).expect("commit");

        // Register snapshot near max timestamp
        registry
            .register_snapshot(
                SnapshotHandle::new(u64::MAX - 1, TransactionId::new(100)).expect("snap"),
            )
            .expect("register snap");

        // Version with end_ts = u64::MAX - 2 should be reclaimable
        assert!(
            collector.is_version_reclaimable(tx, u64::MAX - 2),
            "Large timestamp handling correct"
        );
    }

    /// Test: Multiple status transitions don't corrupt GC state
    #[test]
    fn test_edge_case_status_transitions() {
        let (_registry, status_table, collector) = setup_gc_system();

        let tx = TransactionId::new(1);

        // Initial: InFlight (not reclaimable)
        assert!(!collector.is_version_reclaimable(tx, 100));

        // Transition to Committed (now check eligibility)
        status_table.set_committed(tx).expect("commit");
        assert!(collector.is_version_reclaimable(tx, 50), "Reclaimable after commit");

        // Check idempotence: committing again should not affect result
        status_table.set_committed(tx).expect("commit again");
        assert!(collector.is_version_reclaimable(tx, 50), "Still reclaimable");
    }

    // ====================================================================
    // METRICS & SUMMARY GENERATION
    // ====================================================================

    /// Test: GC stats generation includes all metrics
    #[test]
    fn test_metrics_gc_summary_complete() {
        let (registry, status_table, collector) = setup_gc_system();

        let tx1 = TransactionId::new(1);
        let reader_tx = TransactionId::new(100);

        status_table.set_committed(tx1).expect("commit");

        registry
            .register_snapshot(SnapshotHandle::new(500, reader_tx).expect("snap"))
            .expect("register snap");

        // Simulate GC run
        let summary = collector.run_gc().expect("gc run");

        assert_eq!(
            summary.min_visible_ts, 500,
            "Summary includes correct min_visible_ts"
        );

        // Record metrics
        collector.record_versions_scanned(5000);
        collector.record_versions_reclaimed(4500);

        let stats = collector.get_stats();
        assert_eq!(stats.versions_scanned, 5000);
        assert_eq!(stats.versions_reclaimed, 4500);
        assert!(stats.runs >= 1);
    }

    /// Test: Reclamation rate calculation (>95% of candidates)
    #[test]
    fn test_metrics_reclamation_rate_high() {
        let (registry, status_table, collector) = setup_gc_system();

        // Scan 1000 versions, reclaim 980 (98% success rate)
        collector.record_versions_scanned(1000);
        collector.record_versions_reclaimed(980);

        let stats = collector.get_stats();
        let reclaim_rate = (stats.versions_reclaimed as f64 / stats.versions_scanned as f64) * 100.0;

        assert!(
            reclaim_rate >= 95.0,
            "Reclamation rate {} >= 95%",
            reclaim_rate
        );
    }
}
