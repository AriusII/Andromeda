//! Metrics and summary generation gates for MVCC GC.

use super::fixtures::*;
use andromeda_types::TransactionId;

#[test]
fn test_metrics_gc_summary_complete() {
    let (registry, status_table, collector) = setup_gc_system();

    let tx1 = TransactionId::new(1);
    let reader_tx = TransactionId::new(100);

    record_committed(&status_table, tx1);
    register_snapshot(&registry, 500, reader_tx);

    let summary = collector.run_gc().expect("gc run");

    assert_eq!(
        summary.min_visible_ts, 500,
        "Summary includes correct min_visible_ts"
    );

    collector.record_versions_scanned(5000);
    collector.record_versions_reclaimed(4500);

    let stats = collector.get_stats();
    assert_eq!(stats.versions_scanned, 5000);
    assert_eq!(stats.versions_reclaimed, 4500);
    assert!(stats.runs >= 1);
}

#[test]
fn test_metrics_reclamation_rate_high() {
    let (_registry, _status_table, collector) = setup_gc_system();

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
