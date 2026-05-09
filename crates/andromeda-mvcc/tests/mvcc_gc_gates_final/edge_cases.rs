//! Edge-case gates for transaction IDs, large timestamps, and status transitions.

use super::fixtures::*;
use andromeda_core::TransactionId;

#[test]
fn test_edge_case_zero_transaction_id() {
    let (_registry, _status_table, collector) = setup_gc_system();

    let tx_zero = TransactionId::new(0);
    let result = collector.is_version_reclaimable(tx_zero, 100);
    assert!(!result, "Zero tx_id not reclaimable (never committed)");
}

#[test]
fn test_edge_case_large_timestamps() {
    let (registry, status_table, collector) = setup_gc_system();

    let tx = TransactionId::new(1);
    record_committed(&status_table, tx);
    register_snapshot(&registry, u64::MAX - 1, TransactionId::new(100));

    assert!(
        collector.is_version_reclaimable(tx, u64::MAX - 2),
        "Large timestamp handling correct"
    );
}

#[test]
fn test_edge_case_status_transitions() {
    let (_registry, status_table, collector) = setup_gc_system();

    let tx = TransactionId::new(1);

    assert!(!collector.is_version_reclaimable(tx, 100));

    record_committed(&status_table, tx);
    assert!(
        collector.is_version_reclaimable(tx, 50),
        "Reclaimable after commit"
    );

    record_committed(&status_table, tx);
    assert!(
        collector.is_version_reclaimable(tx, 50),
        "Still reclaimable"
    );
}
