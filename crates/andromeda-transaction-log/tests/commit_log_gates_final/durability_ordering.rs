//! Durability and LSN ordering gates.

use super::fixtures::*;
use andromeda_transaction_log::{IsolationLevel, Lsn, WalRecordKind};
use andromeda_types::TransactionId;

#[tokio::test]
async fn test_durability_crash_before_flush_invisible() {
    let (_wal, _status_table, commit_log) = setup_commit_log();

    let tx_id = TransactionId::new(1);

    let result = commit_log
        .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
        .await;

    assert!(result.is_ok());
    assert!(commit_log.is_committed(tx_id));
}

#[tokio::test]
async fn test_durability_crash_after_flush_visible() {
    let (wal, _status_table, commit_log) = setup_commit_log();

    let tx_id = TransactionId::new(1);
    let _entry = commit_log
        .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
        .await
        .expect("commit");

    let durable_lsn = wal.get_durable_lsn();
    assert!(durable_lsn.get() > 0, "WAL was flushed durably (LSN > 0)");

    let durability_check = commit_log.verify_durability(tx_id);
    assert!(durability_check.is_ok(), "Transaction is durable");
}

#[tokio::test]
async fn test_durability_lsn_consistency() {
    let (wal, _status_table, commit_log) = setup_commit_log();

    let tx_id = TransactionId::new(1);
    let entry = commit_log
        .record_commit(tx_id, IsolationLevel::Snapshot, 5, 0)
        .await
        .expect("commit");

    let records = wal.get_records();
    assert_eq!(records.len(), 1, "Exactly one WAL record");
    assert_eq!(records[0].0, WalRecordKind::TxCommit, "Record is TxCommit");
    assert_eq!(entry.commit_lsn, Lsn::new(1), "Entry LSN = 1");
    assert_eq!(wal.get_durable_lsn(), Lsn::new(1), "Durable LSN = 1");
}

#[tokio::test]
async fn test_ordering_lsn_monotonic_increasing() {
    let (_wal, _status_table, commit_log) = setup_commit_log();

    let mut lsns = vec![];
    for i in 1..=10 {
        let tx_id = TransactionId::new(i as u64);
        let entry = commit_log
            .record_commit(tx_id, IsolationLevel::Snapshot, i as u64, 0)
            .await
            .unwrap_or_else(|_| panic!("commit tx {}", i));

        lsns.push(entry.commit_lsn);
    }

    for i in 1..lsns.len() {
        assert!(
            lsns[i].get() > lsns[i - 1].get(),
            "LSN {} > LSN {} (monotonic increasing)",
            lsns[i].get(),
            lsns[i - 1].get()
        );
    }
}

#[tokio::test]
async fn test_ordering_no_commit_without_wal() {
    let (wal, _status_table, commit_log) = setup_commit_log();

    let tx_id = TransactionId::new(1);

    assert_eq!(wal.record_count(), 0);

    let entry = commit_log
        .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
        .await
        .expect("commit");

    assert_eq!(wal.record_count(), 1, "One WAL record per commit");
    assert_eq!(entry.commit_lsn, Lsn::new(1), "Entry LSN matches WAL LSN");
}
