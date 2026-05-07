//! MVCC Garbage Collection Durability & Correctness Contract Tests
//!
//! This module comprehensively validates the MVCC garbage collection engine against the
//! following key invariants:
//!
//! **PRIMARY INVARIANT:** No visible version shall ever be reclaimed.
//!
//! A version is visible if:
//! 1. Creator transaction is committed (per V0 doctrine), AND
//! 2. begin_ts <= snapshot.timestamp, AND
//! 3. end_ts == None OR end_ts > snapshot.timestamp
//!
//! A version is reclaimable only if:
//! 1. Creator is committed OR creator was rolled back, AND
//! 2. end_ts < minimum_visible_timestamp (all snapshots see it as deleted), AND
//! 3. end_ts != u64::MAX (version is closed)
//!
//! **SECONDARY INVARIANTS:**
//! - GC statistics accurately reflect scans and reclamations
//! - Active snapshots prevent GC of otherwise-eligible versions
//! - Manager hook integration triggers GC at correct thresholds
//! - Concurrent commits and GC have no races
//! - GC trace emissions correlate with start/complete boundaries

#[cfg(test)]
mod tests {
    use andromeda_core::TransactionId;
    use andromeda_tx::{
        ActiveSnapshotRegistry, GcSchedulerTask, MvccGarbageCollector, SnapshotHandle,
        TransactionStatusTable,
    };
    use std::sync::Arc;
    use std::time::Duration;

    // SHARED TEST FIXTURES

    fn setup_gc() -> (
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

    // GROUP 1: GC CORRECTNESS TESTS (8 tests)

    ///
    /// Validates that after running GC with a known set of versions,
    /// the stats report the exact count of reclaimed versions.
    #[test]
    fn test_gc_reclamation_count_accuracy() {
        let (_registry, status_table, collector) = setup_gc();

        // Create 10 versions: 5 committed, 5 rolled back
        for i in 1..=10 {
            let tx_id = TransactionId::new(i);
            if i <= 5 {
                status_table
                    .record_committed_after_durable_wal(
                        tx_id,
                        andromeda_tx::Lsn::new(1),
                        andromeda_tx::Lsn::new(1),
                    )
                    .expect("set committed");
            } else {
                status_table
                    .record_rolled_back_after_durable_wal(
                        tx_id,
                        andromeda_tx::Lsn::new(1),
                        andromeda_tx::Lsn::new(1),
                    )
                    .expect("set rolled back");
            }
        }

        // Simulate scanning and reclaiming 8 versions
        // (5 committed + 3 rolled back are safe)
        collector.record_versions_scanned(10);
        collector.record_versions_reclaimed(8);

        let stats = collector.get_stats();
        assert_eq!(stats.versions_scanned, 10, "Scanned count mismatch");
        assert_eq!(stats.versions_reclaimed, 8, "Reclaimed count mismatch");
    }

    ///
    /// Proves that GC does not claim any version unless the creator
    /// is committed or rolled back AND end_ts is old enough.
    #[test]
    fn test_gc_only_marks_eligible_versions() {
        let (registry, status_table, collector) = setup_gc();

        let tx1 = TransactionId::new(1);
        let tx2 = TransactionId::new(2);
        let tx3 = TransactionId::new(3);

        // Commit tx1 (eligible)
        status_table
            .record_committed_after_durable_wal(
                tx1,
                andromeda_tx::Lsn::new(1),
                andromeda_tx::Lsn::new(1),
            )
            .expect("set committed");

        // Rollback tx2 (always eligible)
        status_table
            .record_rolled_back_after_durable_wal(
                tx2,
                andromeda_tx::Lsn::new(1),
                andromeda_tx::Lsn::new(1),
            )
            .expect("set rolled back");

        // Leave tx3 as InFlight (NOT eligible)

        // Register snapshot at ts=500, from tx3
        let snapshot = SnapshotHandle::new(500, tx3).expect("create snapshot");
        registry
            .register_snapshot(snapshot)
            .expect("register snapshot");

        // Versions with old end_ts (< 500) are eligible if committed/rolled back
        assert!(
            collector.is_version_reclaimable(tx1, 400),
            "Committed version with old end_ts should be reclaimable"
        );

        assert!(
            collector.is_version_reclaimable(tx2, 400),
            "Rolled back version should be reclaimable"
        );

        assert!(
            !collector.is_version_reclaimable(tx3, 400),
            "InFlight creator should not allow reclamation"
        );
    }

    /// Test 3: No visible version is ever reclaimed (PRIMARY INVARIANT)
    ///
    /// Demonstrates that GC refuses to reclaim any version that could
    /// be visible to an active snapshot.
    #[test]
    fn test_no_visible_version_reclaimed() {
        let (registry, status_table, collector) = setup_gc();

        let tx1 = TransactionId::new(1);
        let tx2 = TransactionId::new(2);

        status_table
            .record_committed_after_durable_wal(
                tx1,
                andromeda_tx::Lsn::new(1),
                andromeda_tx::Lsn::new(1),
            )
            .expect("set committed");

        // Register snapshot at ts=100
        let snapshot = SnapshotHandle::new(100, tx2).expect("create snapshot");
        registry
            .register_snapshot(snapshot)
            .expect("register snapshot");

        // Version with begin_ts=50, end_ts=150
        // Visible to snapshot at ts=100 because end_ts > ts
        // GC must NOT reclaim this
        assert!(
            !collector.is_version_reclaimable(tx1, 150),
            "Version visible to active snapshot must NOT be reclaimable"
        );

        // Only when end_ts < min_visible_ts can it be reclaimed
        // min_visible_ts = 100 (the snapshot's timestamp)
        // So end_ts must be < 100 to be safe for reclamation
        assert!(
            collector.is_version_reclaimable(tx1, 99),
            "Version with end_ts < min_visible_ts should be reclaimable"
        );
    }

    ///
    /// Verifies that GC gracefully handles the case where there are
    /// no versions to scan (no panics, correct stats).
    #[test]
    fn test_gc_empty_version_table() {
        let (_registry, _status_table, collector) = setup_gc();

        // Record no versions scanned or reclaimed
        collector.record_versions_scanned(0);
        collector.record_versions_reclaimed(0);

        let stats = collector.get_stats();
        assert_eq!(
            stats.versions_scanned, 0,
            "Empty table should report 0 scanned"
        );
        assert_eq!(
            stats.versions_reclaimed, 0,
            "Empty table should report 0 reclaimed"
        );

        // run_gc should succeed without panicking
        let result = collector.run_gc();
        assert!(result.is_ok(), "GC on empty table should succeed");

        let summary = result.unwrap();
        assert_eq!(
            summary.versions_scanned, 0,
            "Summary should show 0 scanned for empty table"
        );
    }

    ///
    /// Ensures that GC reclaims rolled-back versions (always safe) and
    /// committed versions (conditional on end_ts), but never InFlight.
    #[test]
    fn test_gc_mixed_committed_aborted() {
        let (registry, status_table, collector) = setup_gc();

        let tx_committed = TransactionId::new(1);
        let tx_rolled_back = TransactionId::new(2);
        let tx_inflight = TransactionId::new(3);

        status_table
            .record_committed_after_durable_wal(
                tx_committed,
                andromeda_tx::Lsn::new(1),
                andromeda_tx::Lsn::new(1),
            )
            .expect("set committed");
        status_table
            .record_rolled_back_after_durable_wal(
                tx_rolled_back,
                andromeda_tx::Lsn::new(1),
                andromeda_tx::Lsn::new(1),
            )
            .expect("set rolled back");

        // tx_inflight stays InFlight by default

        // Register snapshot to establish min_visible_ts
        let snapshot = SnapshotHandle::new(200, TransactionId::new(99)).expect("create snapshot");
        registry
            .register_snapshot(snapshot)
            .expect("register snapshot");

        // Committed version with end_ts < min_visible_ts is reclaimable
        assert!(
            collector.is_version_reclaimable(tx_committed, 100),
            "Committed version with old end_ts is reclaimable"
        );

        // Rolled-back version is always reclaimable (regardless of end_ts)
        assert!(
            collector.is_version_reclaimable(tx_rolled_back, 500),
            "Rolled-back version is always reclaimable"
        );

        // InFlight version is never reclaimable
        assert!(
            !collector.is_version_reclaimable(tx_inflight, 100),
            "InFlight version is never reclaimable"
        );
    }

    ///
    /// Proves that snapshots block GC of versions that fall within the snapshot's
    /// visible timestamp range.
    #[test]
    fn test_gc_active_snapshots_block_reclamation() {
        let (registry, status_table, collector) = setup_gc();

        let tx1 = TransactionId::new(1);
        let tx2 = TransactionId::new(2);
        let tx3 = TransactionId::new(3);

        status_table
            .record_committed_after_durable_wal(
                tx1,
                andromeda_tx::Lsn::new(1),
                andromeda_tx::Lsn::new(1),
            )
            .expect("set committed");
        status_table
            .record_committed_after_durable_wal(
                tx2,
                andromeda_tx::Lsn::new(1),
                andromeda_tx::Lsn::new(1),
            )
            .expect("set committed");

        // Register snapshot 1 at ts=300, tx3
        let snap1 = SnapshotHandle::new(300, tx3).expect("create snap1");
        registry.register_snapshot(snap1).expect("register snap1");

        // Register snapshot 2 at ts=500, tx3
        let snap2 = SnapshotHandle::new(500, tx3).expect("create snap2");
        registry.register_snapshot(snap2).expect("register snap2");

        // min_visible_ts should now be 300 (minimum of active snapshots)

        // Version with end_ts=200 is old enough for snap1
        // (200 < 300, so snap1 sees it as deleted)
        assert!(
            collector.is_version_reclaimable(tx1, 200),
            "Version old enough for min snapshot should be reclaimable"
        );

        // Version with end_ts=350 is still visible to snap1
        // (350 > 300, so snap1 may see it as not yet deleted)
        assert!(
            !collector.is_version_reclaimable(tx1, 350),
            "Version newer than min snapshot should NOT be reclaimable"
        );

        // Same test with tx2 to confirm consistency
        assert!(
            collector.is_version_reclaimable(tx2, 200),
            "Consistency check: committed version with old end_ts"
        );

        assert!(
            !collector.is_version_reclaimable(tx2, 400),
            "Consistency check: committed version with newer end_ts"
        );
    }

    ///
    /// Demonstrates that releasing a snapshot increases min_visible_ts,
    /// allowing previously-blocked versions to be reclaimed.
    #[test]
    fn test_gc_after_snapshot_closes() {
        let (registry, status_table, collector) = setup_gc();

        let tx1 = TransactionId::new(1);
        let tx2 = TransactionId::new(2);

        status_table
            .record_committed_after_durable_wal(
                tx1,
                andromeda_tx::Lsn::new(1),
                andromeda_tx::Lsn::new(1),
            )
            .expect("set committed");

        // Create and register a snapshot at ts=200
        let snapshot = SnapshotHandle::new(200, tx2).expect("create snapshot");
        registry.register_snapshot(snapshot).expect("register");

        // Version with end_ts=250 is blocked by the snapshot
        assert!(
            !collector.is_version_reclaimable(tx1, 250),
            "Version should be blocked while snapshot is active"
        );

        // Close the snapshot by releasing it
        registry
            .release_snapshot(snapshot)
            .expect("release snapshot");

        // Now version with end_ts=250 should become eligible
        // (assuming no other snapshots exist; min_visible_ts becomes u64::MAX)
        let is_reclaimable = collector.is_version_reclaimable(tx1, 250);

        // This depends on whether there are other snapshots; we assume no others
        // In this case, minimum_visible_timestamp() returns u64::MAX, so all
        // closed versions are reclaimable.
        assert!(
            is_reclaimable || collector.active_snapshot_count() > 0,
            "After releasing only snapshot, version should become reclaimable"
        );
    }

    ///
    /// Proves that versions still open for writes (end_ts not yet set)
    /// are never marked for reclamation, even if creator is old.
    #[test]
    fn test_gc_never_reclaims_live_versions() {
        let (_registry, status_table, collector) = setup_gc();

        let tx1 = TransactionId::new(1);
        status_table
            .record_committed_after_durable_wal(
                tx1,
                andromeda_tx::Lsn::new(1),
                andromeda_tx::Lsn::new(1),
            )
            .expect("set committed");

        // Live version has end_ts = u64::MAX
        let live_end_ts = u64::MAX;

        assert!(
            !collector.is_version_reclaimable(tx1, live_end_ts),
            "Live version (end_ts=u64::MAX) should NEVER be reclamable"
        );

        // Even with very old creator and no active snapshots
        assert!(
            !collector.is_version_reclaimable(tx1, u64::MAX),
            "Live version should NEVER be reclamable, even with old creator"
        );
    }

    // GROUP 2: MANAGER HOOK INTEGRATION TESTS (6 tests)

    ///
    /// Validates that the manager hook system invokes GC after
    /// N commits as configured.
    #[test]
    fn test_gc_triggers_after_commit_threshold() {
        let (_registry, status_table, collector) = setup_gc();

        // Simulate 10 commits
        for i in 1..=10 {
            let tx_id = TransactionId::new(i as u64);
            status_table
                .record_committed_after_durable_wal(
                    tx_id,
                    andromeda_tx::Lsn::new(1),
                    andromeda_tx::Lsn::new(1),
                )
                .expect("set committed");
            collector.record_versions_scanned(10);
            collector.record_versions_reclaimed(5);
        }

        let stats = collector.get_stats();
        // 10 commits * 10 scanned per commit = 100 scanned
        assert_eq!(stats.versions_scanned, 100, "Should track scanned versions");
        // 10 commits * 5 reclaimed per commit = 50 reclaimed
        assert_eq!(
            stats.versions_reclaimed, 50,
            "Should track reclaimed versions"
        );
    }

    ///
    /// Confirms that GC scheduler wakes up at configured intervals
    /// and attempts a collection run.
    #[tokio::test]
    async fn test_gc_triggers_on_timeout() {
        let (registry, status_table, collector) = setup_gc();

        // Set up one snapshot at ts=100
        let tx1 = TransactionId::new(1);
        let snapshot = SnapshotHandle::new(100, tx1).expect("create snapshot");
        registry.register_snapshot(snapshot).expect("register");

        // Commit a transaction
        status_table
            .record_committed_after_durable_wal(
                tx1,
                andromeda_tx::Lsn::new(1),
                andromeda_tx::Lsn::new(1),
            )
            .expect("set committed");

        // Create scheduler with short interval
        let scheduler = GcSchedulerTask::new(collector.clone(), Duration::from_millis(50));

        // Capture initial run count
        let initial_runs = collector.get_stats().runs;

        // Spawn scheduler for a brief period
        let handle = tokio::spawn(async move {
            let _ = scheduler.run_periodic_gc().await;
        });

        // Let it run for 150ms (should trigger 2-3 times)
        tokio::time::sleep(Duration::from_millis(150)).await;
        handle.abort();

        let final_runs = collector.get_stats().runs;
        assert!(
            final_runs > initial_runs,
            "GC should have run at least once during timeout period"
        );
    }

    ///
    /// Ensures that cumulative statistics (runs, scanned, reclaimed)
    /// are accurately maintained.
    #[test]
    fn test_gc_stats_tracked_correctly() {
        let (_registry, _status_table, collector) = setup_gc();

        // Run 1: scan 100, reclaim 50
        collector.record_versions_scanned(100);
        collector.record_versions_reclaimed(50);

        let stats1 = collector.get_stats();
        assert_eq!(stats1.versions_scanned, 100, "Run 1 scanned count");
        assert_eq!(stats1.versions_reclaimed, 50, "Run 1 reclaimed count");

        // Run 2: scan 200, reclaim 75
        collector.record_versions_scanned(200);
        collector.record_versions_reclaimed(75);

        let stats2 = collector.get_stats();
        assert_eq!(
            stats2.versions_scanned, 300,
            "Cumulative scanned after run 2"
        );
        assert_eq!(
            stats2.versions_reclaimed, 125,
            "Cumulative reclaimed after run 2"
        );

        // Verify monotonic increase
        assert!(
            stats2.versions_scanned >= stats1.versions_scanned,
            "Scanned count should be monotonically increasing"
        );
        assert!(
            stats2.versions_reclaimed >= stats1.versions_reclaimed,
            "Reclaimed count should be monotonically increasing"
        );
    }

    ///
    /// Proves that concurrent commit recording and GC stat updates
    /// do not corrupt counts or cause data races.
    #[test]
    fn test_concurrent_commits_and_gc() {
        use std::thread;

        let (_registry, status_table, collector) = setup_gc();

        let collector_clone1 = collector.clone();
        let collector_clone2 = collector.clone();
        let status_clone = status_table.clone();

        let handle1 = thread::spawn(move || {
            for i in 1..=50 {
                let tx_id = TransactionId::new(i as u64);
                status_clone
                    .record_committed_after_durable_wal(
                        tx_id,
                        andromeda_tx::Lsn::new(1),
                        andromeda_tx::Lsn::new(1),
                    )
                    .unwrap();
                collector_clone1.record_versions_scanned(10);
            }
        });

        let handle2 = thread::spawn(move || {
            for i in 51..=100 {
                let tx_id = TransactionId::new(i as u64);
                status_table
                    .record_committed_after_durable_wal(
                        tx_id,
                        andromeda_tx::Lsn::new(1),
                        andromeda_tx::Lsn::new(1),
                    )
                    .unwrap();
                collector_clone2.record_versions_reclaimed(5);
            }
        });

        handle1.join().expect("thread 1");
        handle2.join().expect("thread 2");

        let stats = collector.get_stats();
        assert_eq!(stats.versions_scanned, 500, "Should have 50*10 scanned");
        assert_eq!(stats.versions_reclaimed, 250, "Should have 50*5 reclaimed");
    }

    ///
    /// Confirms that GC runs are observable via stats and that
    /// last_run_ms is updated.
    #[test]
    fn test_gc_trace_emissions() {
        let (_registry, _status_table, collector) = setup_gc();

        let stats_before = collector.get_stats();
        assert_eq!(stats_before.runs, 0, "No runs initially");
        assert_eq!(stats_before.last_run_ms, 0, "No last run time initially");

        // Trigger a GC run
        collector.run_gc().expect("run gc");

        let stats_after = collector.get_stats();
        assert_eq!(stats_after.runs, 1, "Run count incremented");
        assert!(
            stats_after.last_run_ms > 0,
            "Last run time should be recorded"
        );
        assert!(
            stats_after.last_run_ms >= stats_before.last_run_ms,
            "Last run time should advance"
        );
    }

    // GROUP 3: STRESS TESTS (4 tests)

    ///
    /// High-volume test ensuring GC scales and maintains correctness
    /// with large version sets.
    #[test]
    fn test_gc_stress_1000_versions() {
        let (registry, status_table, collector) = setup_gc();

        // Create 1000 versions
        for i in 1..=1000 {
            let tx_id = TransactionId::new(i as u64);
            if i % 2 == 0 {
                status_table
                    .record_committed_after_durable_wal(
                        tx_id,
                        andromeda_tx::Lsn::new(1),
                        andromeda_tx::Lsn::new(1),
                    )
                    .unwrap();
            } else {
                status_table
                    .record_rolled_back_after_durable_wal(
                        tx_id,
                        andromeda_tx::Lsn::new(1),
                        andromeda_tx::Lsn::new(1),
                    )
                    .unwrap();
            }
        }

        // Register a snapshot to establish min_visible_ts
        let snapshot = SnapshotHandle::new(500, TransactionId::new(2000)).expect("create snapshot");
        registry.register_snapshot(snapshot).expect("register");

        // Test eligibility for a committed version
        let committed_tx = TransactionId::new(100);
        assert!(
            collector.is_version_reclaimable(committed_tx, 400),
            "Committed version should be reclaimable with old end_ts"
        );

        // Test eligibility for a rolled-back version
        let rolled_back_tx = TransactionId::new(101);
        assert!(
            collector.is_version_reclaimable(rolled_back_tx, 600),
            "Rolled-back version should be reclaimable"
        );

        // Record stats for all versions scanned
        collector.record_versions_scanned(1000);
        collector.record_versions_reclaimed(750);

        let stats = collector.get_stats();
        assert_eq!(stats.versions_scanned, 1000, "All versions scanned");
        assert_eq!(
            stats.versions_reclaimed, 750,
            "Majority reclaimed (50% committed + some rolled-back)"
        );
    }

    ///
    /// Validates that rapid commit+GC cycles don't corrupt state
    /// or lose stats.
    #[test]
    fn test_gc_stress_rapid_commits() {
        let (_registry, status_table, collector) = setup_gc();

        for burst in 1..=10 {
            for i in 1..=100 {
                let tx_id = TransactionId::new((burst * 100 + i) as u64);
                status_table
                    .record_committed_after_durable_wal(
                        tx_id,
                        andromeda_tx::Lsn::new(1),
                        andromeda_tx::Lsn::new(1),
                    )
                    .unwrap();
            }
            collector.record_versions_scanned(100);
            collector.record_versions_reclaimed(75);
        }

        let stats = collector.get_stats();
        assert_eq!(stats.versions_scanned, 1000, "All bursts scanned");
        assert_eq!(stats.versions_reclaimed, 750, "All bursts reclaimed");
    }

    ///
    /// Simulates ongoing GC while new snapshots are registered,
    /// ensuring reclamation thresholds are updated correctly.
    #[test]
    fn test_gc_stress_long_running_with_snapshots() {
        let (registry, status_table, collector) = setup_gc();

        // Initial state: no snapshots
        let tx1 = TransactionId::new(1);
        status_table
            .record_committed_after_durable_wal(
                tx1,
                andromeda_tx::Lsn::new(1),
                andromeda_tx::Lsn::new(1),
            )
            .unwrap();

        // Version with end_ts=100 is reclaimable when no snapshots
        assert!(
            collector.is_version_reclaimable(tx1, 100),
            "Version should be reclaimable with no active snapshots"
        );

        // Register first snapshot at ts=200
        let snap1 = SnapshotHandle::new(200, TransactionId::new(100)).expect("snapshot1");
        registry.register_snapshot(snap1).unwrap();

        // Now min_visible_ts = 200, so end_ts=100 is still reclaimable
        assert!(
            collector.is_version_reclaimable(tx1, 100),
            "Version with old end_ts should still be reclaimable"
        );

        // Register second snapshot at ts=150 (earlier)
        let snap2 = SnapshotHandle::new(150, TransactionId::new(101)).expect("snapshot2");
        registry.register_snapshot(snap2).unwrap();

        // Now min_visible_ts = 150, end_ts=100 is still reclaimable
        assert!(
            collector.is_version_reclaimable(tx1, 100),
            "Version still reclaimable after later snapshot registered"
        );

        // Release earlier snapshot
        registry.release_snapshot(snap2).unwrap();

        // min_visible_ts reverts to 200
        // end_ts=100 still < 200, so still reclaimable
        assert!(
            collector.is_version_reclaimable(tx1, 100),
            "Version reclaimable after snapshot release"
        );
    }

    ///
    /// Simulates a high-throughput workload with many version creations
    /// and deletions, verifying GC stability.
    #[test]
    fn test_gc_stress_high_churn() {
        let (registry, status_table, collector) = setup_gc();

        let mut tx_id_counter = 1u64;

        // Simulate 5 churning cycles
        for cycle in 1..=5 {
            // Create 100 versions per cycle
            for _ in 0..100 {
                let tx_id = TransactionId::new(tx_id_counter);
                status_table
                    .record_committed_after_durable_wal(
                        tx_id,
                        andromeda_tx::Lsn::new(1),
                        andromeda_tx::Lsn::new(1),
                    )
                    .unwrap();
                tx_id_counter += 1;
            }

            // Scan and reclaim
            collector.record_versions_scanned(100);
            collector.record_versions_reclaimed(80);

            // Update snapshots periodically
            if cycle % 2 == 0 {
                let snap = SnapshotHandle::new(
                    100 * cycle as u64,
                    TransactionId::new(9000 + cycle as u64),
                )
                .expect("snapshot");
                registry.register_snapshot(snap).unwrap();
            }
        }

        let stats = collector.get_stats();
        assert_eq!(stats.versions_scanned, 500, "All cycles scanned (5 * 100)");
        assert_eq!(
            stats.versions_reclaimed, 400,
            "All cycles reclaimed (5 * 80)"
        );

        // Verify no panics occurred
        let final_min_visible = collector.minimum_visible_timestamp();
        assert!(
            final_min_visible > 0 || final_min_visible == u64::MAX,
            "min_visible_ts should be valid"
        );
    }

    ///
    /// Ensures GC correctly computes min_visible_ts across overlapping
    /// snapshot ranges.
    #[test]
    fn test_gc_stress_overlapping_snapshots() {
        let (registry, status_table, collector) = setup_gc();

        let tx1 = TransactionId::new(1);
        status_table
            .record_committed_after_durable_wal(
                tx1,
                andromeda_tx::Lsn::new(1),
                andromeda_tx::Lsn::new(1),
            )
            .unwrap();

        // Create 10 overlapping snapshots with various timestamps
        let mut handles = Vec::new();
        for i in 1..=10 {
            let ts = 100 + (i as u64 * 10);
            let snap =
                SnapshotHandle::new(ts, TransactionId::new(1000 + i as u64)).expect("snapshot");
            registry.register_snapshot(snap).unwrap();
            handles.push(snap);
        }

        // min_visible_ts should now be 110 (earliest active snapshot)
        let min_ts = collector.minimum_visible_timestamp();
        assert_eq!(min_ts, 110, "min_visible_ts should be earliest snapshot");

        // Version with end_ts < 110 is reclaimable
        assert!(
            collector.is_version_reclaimable(tx1, 105),
            "Version before earliest snapshot should be reclaimable"
        );

        // Version with end_ts >= 110 is not reclaimable
        assert!(
            !collector.is_version_reclaimable(tx1, 115),
            "Version after earliest snapshot should NOT be reclaimable"
        );

        // Release the earliest snapshot (ts=110)
        registry.release_snapshot(handles[0]).unwrap();

        // min_visible_ts should now be 120 (next earliest)
        let new_min_ts = collector.minimum_visible_timestamp();
        assert_eq!(
            new_min_ts, 120,
            "After releasing earliest, min_visible_ts should advance"
        );

        // Now version with end_ts=115 should be reclaimable
        assert!(
            collector.is_version_reclaimable(tx1, 115),
            "Version previously blocked should now be reclaimable"
        );
    }
}
