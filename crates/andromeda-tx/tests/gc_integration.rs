//! Comprehensive integration tests for MVCC Garbage Collection
//!
//! Tests validate the full GC lifecycle: eligibility determination, scheduling,
//! and integration with active snapshots and transaction state.

#[cfg(test)]
mod tests {
    use andromeda_core::TransactionId;
    use andromeda_tx::{
        ActiveSnapshotRegistry, GcSchedulerTask, MvccGarbageCollector, SnapshotHandle,
        TransactionStatus, TransactionStatusTable,
    };
    use std::sync::Arc;
    use std::time::Duration;

    fn setup_collector() -> (
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

    #[test]
    fn test_gc_reclaims_invisible_versions() {
        let (registry, mut status_table, collector) = setup_collector();

        let tx1 = TransactionId::new(1);
        let tx2 = TransactionId::new(2);

        // Mark tx1 as committed
        status_table
            .record(tx1, TransactionStatus::Committed)
            .expect("record tx1 commit");

        // Register snapshot at ts=200 (from tx2)
        registry
            .register_snapshot(SnapshotHandle::new(200, tx2).expect("snapshot"))
            .expect("register snapshot");

        // Version created by tx1 with end_ts=150 should be reclaimable
        // (150 < 200 = min_visible_ts)
        assert!(
            collector.is_version_reclaimable(tx1, 150),
            "Version with end_ts=150 should be reclaimable when min_visible_ts=200"
        );
    }

    #[test]
    fn test_gc_preserves_visible_versions() {
        let (registry, mut status_table, collector) = setup_collector();

        let tx1 = TransactionId::new(1);
        let tx2 = TransactionId::new(2);

        status_table
            .record(tx1, TransactionStatus::Committed)
            .expect("record");

        // Register snapshot at ts=100
        registry
            .register_snapshot(SnapshotHandle::new(100, tx2).expect("snapshot"))
            .expect("register");

        // Version with end_ts=100 should NOT be reclaimable (on boundary)
        assert!(
            !collector.is_version_reclaimable(tx1, 100),
            "Version with end_ts=100 should NOT be reclaimable when min_visible_ts=100"
        );

        // Version with end_ts=101 should NOT be reclaimable (newer than min)
        assert!(
            !collector.is_version_reclaimable(tx1, 101),
            "Version with end_ts=101 should NOT be reclaimable"
        );
    }

    #[test]
    fn test_gc_with_multiple_snapshots_uses_minimum() {
        let (registry, mut status_table, collector) = setup_collector();

        let tx1 = TransactionId::new(1);
        let tx2 = TransactionId::new(2);
        let tx3 = TransactionId::new(3);
        let tx4 = TransactionId::new(4);

        status_table
            .record(tx1, TransactionStatus::Committed)
            .expect("record");

        // Register three snapshots at different timestamps
        registry
            .register_snapshot(SnapshotHandle::new(500, tx2).expect("snapshot"))
            .expect("register s1");
        registry
            .register_snapshot(SnapshotHandle::new(200, tx3).expect("snapshot"))
            .expect("register s2");
        registry
            .register_snapshot(SnapshotHandle::new(800, tx4).expect("snapshot"))
            .expect("register s3");

        // Minimum visible should be 200
        assert_eq!(
            collector.minimum_visible_timestamp(),
            200,
            "Minimum visible timestamp should be 200"
        );

        // Version with end_ts=199 should be reclaimable
        assert!(
            collector.is_version_reclaimable(tx1, 199),
            "Version with end_ts=199 should be reclaimable (< 200)"
        );

        // Version with end_ts=200 should NOT be reclaimable
        assert!(
            !collector.is_version_reclaimable(tx1, 200),
            "Version with end_ts=200 should NOT be reclaimable (= min_visible_ts)"
        );
    }

    #[test]
    fn test_gc_after_snapshot_release() {
        let (registry, mut status_table, collector) = setup_collector();

        let tx1 = TransactionId::new(1);
        let tx2 = TransactionId::new(2);
        let tx3 = TransactionId::new(3);

        status_table
            .record(tx1, TransactionStatus::Committed)
            .expect("record");

        let snap2 = SnapshotHandle::new(200, tx2).expect("snapshot");
        let snap3 = SnapshotHandle::new(300, tx3).expect("snapshot");

        registry.register_snapshot(snap2).expect("register s2");
        registry.register_snapshot(snap3).expect("register s3");

        // Before releasing snap2, min_visible = 200
        assert!(
            !collector.is_version_reclaimable(tx1, 200),
            "Before release, version at 200 should not be reclaimable"
        );

        // Release the earlier snapshot
        registry.release_snapshot(snap2).expect("release");

        // Now min_visible = 300
        assert!(
            collector.is_version_reclaimable(tx1, 200),
            "After releasing snap2, version at 200 should be reclaimable"
        );
        assert!(
            collector.is_version_reclaimable(tx1, 299),
            "Version at 299 should be reclaimable"
        );
        assert!(
            !collector.is_version_reclaimable(tx1, 300),
            "Version at 300 should NOT be reclaimable"
        );
    }

    #[test]
    fn test_gc_live_versions_never_reclaimable() {
        let (registry, mut status_table, collector) = setup_collector();

        let tx1 = TransactionId::new(1);
        let tx2 = TransactionId::new(2);

        status_table
            .record(tx1, TransactionStatus::Committed)
            .expect("record");

        registry
            .register_snapshot(SnapshotHandle::new(100, tx2).expect("snapshot"))
            .expect("register");

        // Live version (end_ts = u64::MAX) should never be reclaimable
        assert!(
            !collector.is_version_reclaimable(tx1, u64::MAX),
            "Live version (end_ts=u64::MAX) should never be reclaimable"
        );
    }

    #[test]
    fn test_gc_rejects_in_flight_versions() {
        let (registry, _status_table, collector) = setup_collector();

        let tx1 = TransactionId::new(1);
        let tx2 = TransactionId::new(2);

        registry
            .register_snapshot(SnapshotHandle::new(100, tx2).expect("snapshot"))
            .expect("register");

        // InFlight version (no status record) should never be reclaimable
        // even if end_ts < min_visible_ts (doctrine: no durable commit evidence)
        assert!(
            !collector.is_version_reclaimable(tx1, 50),
            "InFlight version should not be reclaimable, even if old"
        );
    }

    #[test]
    fn test_gc_always_reclaims_rolled_back_versions() {
        let (registry, mut status_table, collector) = setup_collector();

        let tx1 = TransactionId::new(1);
        let tx2 = TransactionId::new(2);

        // Mark tx1 as rolled back
        status_table
            .record(tx1, TransactionStatus::RolledBack)
            .expect("record rollback");

        // No snapshots registered (min_visible = u64::MAX)
        registry
            .register_snapshot(SnapshotHandle::new(u64::MAX - 1, tx2).expect("snapshot"))
            .expect("register");

        // Rolled-back versions are always reclaimable
        assert!(
            collector.is_version_reclaimable(tx1, 1),
            "Rolled-back version should be reclaimable"
        );
        assert!(
            collector.is_version_reclaimable(tx1, 1_000_000),
            "Rolled-back version should be reclaimable regardless of end_ts"
        );

        // Even live versions of rolled-back transactions are reclaimable
        // (though they shouldn't exist in practice)
        assert!(
            !collector.is_version_reclaimable(tx1, u64::MAX),
            "Live version is never reclaimable (even if creator is rolled back)"
        );
    }

    #[test]
    fn test_gc_no_snapshots_means_nothing_reclaimable() {
        let (_registry, mut status_table, collector) = setup_collector();

        let tx1 = TransactionId::new(1);

        status_table
            .record(tx1, TransactionStatus::Committed)
            .expect("record");

        // With no snapshots, min_visible_ts = u64::MAX
        // Therefore, no committed version is reclaimable (even very old ones)
        assert!(
            !collector.is_version_reclaimable(tx1, 1),
            "No versions should be reclaimable when no snapshots are active"
        );
        assert!(
            !collector.is_version_reclaimable(tx1, 1_000_000),
            "Even old versions not reclaimable with no active snapshots"
        );
    }

    #[test]
    fn test_gc_stats_monotonic_accumulation() {
        let (_registry, _status_table, collector) = setup_collector();

        // Initial stats
        let initial = collector.get_stats();
        assert_eq!(initial.versions_scanned, 0);
        assert_eq!(initial.versions_reclaimed, 0);

        // Record some activity
        collector.record_versions_scanned(100);
        let after_scan = collector.get_stats();
        assert_eq!(after_scan.versions_scanned, 100);

        collector.record_versions_reclaimed(25);
        let after_reclaim = collector.get_stats();
        assert_eq!(after_reclaim.versions_scanned, 100);
        assert_eq!(after_reclaim.versions_reclaimed, 25);

        // Add more activity
        collector.record_versions_scanned(50);
        collector.record_versions_reclaimed(10);

        let final_stats = collector.get_stats();
        assert_eq!(final_stats.versions_scanned, 150);
        assert_eq!(final_stats.versions_reclaimed, 35);
    }

    #[test]
    fn test_gc_run_increments_run_counter_and_timestamp() {
        let (_registry, _status_table, collector) = setup_collector();

        let initial = collector.get_stats();
        assert_eq!(initial.runs, 0);
        assert_eq!(initial.last_run_ms, 0);

        collector.run_gc().expect("first gc");

        let after_first = collector.get_stats();
        assert_eq!(after_first.runs, 1);
        assert!(after_first.last_run_ms > 0);

        let first_ts = after_first.last_run_ms;

        // Wait a bit and run again
        std::thread::sleep(Duration::from_millis(10));

        collector.run_gc().expect("second gc");

        let after_second = collector.get_stats();
        assert_eq!(after_second.runs, 2);
        assert!(after_second.last_run_ms >= first_ts);
    }

    #[test]
    fn test_gc_summary_includes_min_visible_ts() {
        let (registry, _status_table, collector) = setup_collector();

        let tx_id = TransactionId::new(1);
        registry
            .register_snapshot(SnapshotHandle::new(500, tx_id).expect("snapshot"))
            .expect("register");

        let summary = collector.run_gc().expect("gc");
        assert_eq!(
            summary.min_visible_ts, 500,
            "GC summary should include min_visible_ts"
        );
    }

    #[test]
    fn test_active_snapshot_count() {
        let (registry, _status_table, collector) = setup_collector();

        assert_eq!(
            collector.active_snapshot_count(),
            0,
            "No snapshots initially"
        );

        let tx1 = TransactionId::new(1);
        let tx2 = TransactionId::new(2);

        registry
            .register_snapshot(SnapshotHandle::new(100, tx1).expect("snapshot"))
            .expect("register");
        assert_eq!(collector.active_snapshot_count(), 1);

        registry
            .register_snapshot(SnapshotHandle::new(200, tx2).expect("snapshot"))
            .expect("register");
        assert_eq!(collector.active_snapshot_count(), 2);
    }

    #[tokio::test]
    async fn test_gc_scheduler_runs_periodically() {
        let (registry, _status_table, collector) = setup_collector();

        let tx_id = TransactionId::new(1);
        registry
            .register_snapshot(SnapshotHandle::new(100, tx_id).expect("snapshot"))
            .expect("register");

        let scheduler = GcSchedulerTask::new(collector.clone(), Duration::from_millis(50));
        let handle = tokio::spawn(scheduler.run_periodic_gc());

        // Let the scheduler run for 300ms
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Cancel the task
        handle.abort();

        // Should have run at least a few times
        let stats = collector.get_stats();
        assert!(
            stats.runs >= 2,
            "Scheduler should have run at least 2 times, got {}",
            stats.runs
        );
    }

    #[test]
    fn test_multiple_transaction_versions() {
        let (registry, mut status_table, collector) = setup_collector();

        // Setup: 3 transactions, different states
        let tx_committed = TransactionId::new(1);
        let tx_in_flight = TransactionId::new(2);
        let tx_rolled_back = TransactionId::new(3);
        let observer_tx = TransactionId::new(4);

        status_table
            .record(tx_committed, TransactionStatus::Committed)
            .expect("record committed");
        status_table
            .record(tx_rolled_back, TransactionStatus::RolledBack)
            .expect("record rollback");
        // tx_in_flight is not recorded

        // Snapshot at ts=200
        registry
            .register_snapshot(SnapshotHandle::new(200, observer_tx).expect("snapshot"))
            .expect("register");

        // Committed version at 150 → should be reclaimable
        assert!(
            collector.is_version_reclaimable(tx_committed, 150),
            "Committed version at 150 should be reclaimable"
        );

        // Committed version at 200 → should NOT be reclaimable (boundary)
        assert!(
            !collector.is_version_reclaimable(tx_committed, 200),
            "Committed version at 200 should NOT be reclaimable"
        );

        // InFlight version → should never be reclaimable
        assert!(
            !collector.is_version_reclaimable(tx_in_flight, 150),
            "InFlight version should never be reclaimable"
        );

        // RolledBack version → should always be reclaimable
        assert!(
            collector.is_version_reclaimable(tx_rolled_back, 150),
            "RolledBack version should be reclaimable"
        );
        assert!(
            collector.is_version_reclaimable(tx_rolled_back, 250),
            "RolledBack version should be reclaimable regardless of end_ts"
        );
    }
}
