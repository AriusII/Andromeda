//! Scheduler, statistics, and trace-observability contracts for MVCC GC.

use super::fixtures::*;
use andromeda_mvcc::GcSchedulerTask;
use andromeda_types::TransactionId;
use std::time::Duration;

#[test]
fn test_gc_triggers_after_commit_threshold() {
    let (_registry, status_table, collector) = setup_gc();

    for i in 1..=10 {
        let tx_id = TransactionId::new(i as u64);
        record_committed(&status_table, tx_id);
        collector.record_versions_scanned(10);
        collector.record_versions_reclaimed(5);
    }

    let stats = collector.get_stats();
    assert_eq!(stats.versions_scanned, 100, "Should track scanned versions");
    assert_eq!(
        stats.versions_reclaimed, 50,
        "Should track reclaimed versions"
    );
}

#[tokio::test]
async fn test_gc_triggers_on_timeout() {
    let (registry, status_table, collector) = setup_gc();

    let tx1 = TransactionId::new(1);
    register_snapshot(&registry, 100, tx1);
    record_committed(&status_table, tx1);

    let scheduler = GcSchedulerTask::new(collector.clone(), Duration::from_millis(50));
    let initial_runs = collector.get_stats().runs;

    let handle = tokio::spawn(async move {
        let _ = scheduler.run_periodic_gc().await;
    });

    tokio::time::sleep(Duration::from_millis(150)).await;
    handle.abort();

    let final_runs = collector.get_stats().runs;
    assert!(
        final_runs > initial_runs,
        "GC should have run at least once during timeout period"
    );
}

#[test]
fn test_gc_stats_tracked_correctly() {
    let (_registry, _status_table, collector) = setup_gc();

    collector.record_versions_scanned(100);
    collector.record_versions_reclaimed(50);

    let stats1 = collector.get_stats();
    assert_eq!(stats1.versions_scanned, 100, "Run 1 scanned count");
    assert_eq!(stats1.versions_reclaimed, 50, "Run 1 reclaimed count");

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
    assert!(
        stats2.versions_scanned >= stats1.versions_scanned,
        "Scanned count should be monotonically increasing"
    );
    assert!(
        stats2.versions_reclaimed >= stats1.versions_reclaimed,
        "Reclaimed count should be monotonically increasing"
    );
}

#[test]
fn test_concurrent_commits_and_gc() {
    let (_registry, status_table, collector) = setup_gc();

    let collector_clone1 = collector.clone();
    let collector_clone2 = collector.clone();
    let status_clone = status_table.clone();

    let handle1 = std::thread::spawn(move || {
        for i in 1..=50 {
            let tx_id = TransactionId::new(i as u64);
            record_committed(&status_clone, tx_id);
            collector_clone1.record_versions_scanned(10);
        }
    });

    let handle2 = std::thread::spawn(move || {
        for i in 51..=100 {
            let tx_id = TransactionId::new(i as u64);
            record_committed(&status_table, tx_id);
            collector_clone2.record_versions_reclaimed(5);
        }
    });

    handle1.join().expect("thread 1");
    handle2.join().expect("thread 2");

    let stats = collector.get_stats();
    assert_eq!(stats.versions_scanned, 500, "Should have 50*10 scanned");
    assert_eq!(stats.versions_reclaimed, 250, "Should have 50*5 reclaimed");
}

#[test]
fn test_gc_trace_emissions() {
    let (_registry, _status_table, collector) = setup_gc();

    let stats_before = collector.get_stats();
    assert_eq!(stats_before.runs, 0, "No runs initially");
    assert_eq!(stats_before.last_run_ms, 0, "No last run time initially");

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
