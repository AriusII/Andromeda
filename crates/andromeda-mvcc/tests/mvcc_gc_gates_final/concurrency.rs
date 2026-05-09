//! Concurrency gates for MVCC GC scheduler and eligibility checks.

use super::fixtures::*;
use andromeda_mvcc::GcSchedulerTask;
use andromeda_types::TransactionId;
use futures::future::join_all;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_concurrency_50_writers_plus_gc_scheduler_task() {
    let (_registry, status_table, collector) = setup_gc_system();
    let collector_clone = collector.clone();
    let status_table_clone = status_table.clone();

    let mut writer_handles = vec![];
    for i in 0..50 {
        let status_table = status_table_clone.clone();
        let collector = collector_clone.clone();

        let handle = tokio::spawn(async move {
            let tx_id = TransactionId::new(1000 + i);

            record_committed(&status_table, tx_id);

            for end_ts in [100, 200, 300, u64::MAX] {
                let _reclaimable = collector.is_version_reclaimable(tx_id, end_ts);
            }

            if i % 5 == 0 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        });

        writer_handles.push(handle);
    }

    let scheduler = Arc::new(GcSchedulerTask::new(
        collector_clone.clone(),
        Duration::from_millis(50),
    ));
    let scheduler_task = scheduler.clone();
    let gc_handle = tokio::spawn(async move { scheduler_task.run_periodic_gc().await });

    let results: Vec<_> = join_all(writer_handles).await;
    for result in results {
        assert!(result.is_ok(), "Writer task panicked");
    }

    tokio::time::sleep(Duration::from_millis(75)).await;
    gc_handle.abort();

    let stats = collector_clone.get_stats();
    assert!(
        stats.runs >= 1,
        "GC should have run at least once during concurrent writes"
    );
}

#[tokio::test]
async fn test_concurrency_gc_safe_with_active_reads() {
    let (registry, status_table, collector) = setup_gc_system();

    let tx1 = TransactionId::new(1);
    let reader_tx = TransactionId::new(100);

    record_committed(&status_table, tx1);

    let snap = register_snapshot(&registry, 300, reader_tx);

    let mut reader_handles = vec![];
    for i in 0..10 {
        let collector = collector.clone();
        let handle = tokio::spawn(async move {
            for _ in 0..100 {
                let _visible_1 = collector.is_version_reclaimable(tx1, 100 + i);
                let _visible_2 = collector.is_version_reclaimable(tx1, 200 + i);
            }
        });
        reader_handles.push(handle);
    }

    let collector_gc = collector.clone();
    let gc_handle = tokio::spawn(async move {
        for _ in 0..10 {
            let _summary = collector_gc.run_gc();
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });

    let _reader_results: Vec<_> = join_all(reader_handles).await;
    let _gc_result = gc_handle.await;

    registry.release_snapshot(snap).expect("release snap");

    let stats = collector.get_stats();
    assert!(stats.runs >= 1, "GC runs completed");
}

#[test]
fn test_concurrency_parallel_eligibility_checks() {
    let (registry, status_table, collector) = setup_gc_system();

    let tx_id = TransactionId::new(1);
    record_committed(&status_table, tx_id);
    register_snapshot(&registry, 1000, TransactionId::new(100));

    let handles: Vec<_> = (0..100)
        .map(|i| {
            let collector = collector.clone();
            std::thread::spawn(move || {
                let end_ts = 100 + (i as u64);
                collector.is_version_reclaimable(tx_id, end_ts)
            })
        })
        .collect();

    for handle in handles {
        let result = handle.join();
        assert!(result.is_ok(), "Thread panicked during concurrent check");
    }
}
