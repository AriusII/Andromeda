//! Isolation metadata, affected-row metadata, and commit-log GC gates.

use super::fixtures::*;
use andromeda_transaction::{IsolationLevel, Lsn};
use andromeda_types::TransactionId;

#[tokio::test]
async fn test_isolation_snapshot_level_recorded() {
    let (_wal, _status_table, commit_log) = setup_commit_log();

    let tx_id = TransactionId::new(1);
    let entry = commit_log
        .record_commit(tx_id, IsolationLevel::Snapshot, 10, 0)
        .await
        .expect("commit");

    assert_eq!(entry.isolation_level, IsolationLevel::Snapshot);
}

#[tokio::test]
async fn test_isolation_serializable_level_recorded() {
    let (_wal, _status_table, commit_log) = setup_commit_log();

    let tx_id = TransactionId::new(2);
    let entry = commit_log
        .record_commit(tx_id, IsolationLevel::Serializable, 5, 0)
        .await
        .expect("commit");

    assert_eq!(entry.isolation_level, IsolationLevel::Serializable);
}

#[tokio::test]
async fn test_gc_candidates_identification() {
    let (_wal, _status_table, commit_log) = setup_commit_log();

    let mut entries = vec![];
    for i in 1..=5 {
        let tx_id = TransactionId::new(i as u64);
        let entry = commit_log
            .record_commit(tx_id, IsolationLevel::Snapshot, 1, 0)
            .await
            .unwrap_or_else(|_| panic!("commit tx {}", i));
        entries.push(entry);
    }

    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(entry.commit_lsn, Lsn::new((i + 1) as u64));
    }

    let gc_candidates = commit_log.gc_candidates(Lsn::new(3));

    assert_eq!(
        gc_candidates.len(),
        2,
        "Two transactions are GC candidates (LSN < 3)"
    );
    assert!(gc_candidates.contains(&TransactionId::new(1)));
    assert!(gc_candidates.contains(&TransactionId::new(2)));

    let removed = commit_log.gc_remove(gc_candidates.into_iter());
    assert_eq!(removed, 2, "Two entries removed");
}

#[tokio::test]
async fn test_edge_case_metadata_preservation() {
    let (_wal, _status_table, commit_log) = setup_commit_log();

    let tx_id = TransactionId::new(200);
    let affected_rows = 12345u64;
    let param_hash = 0xDEADBEEFu64;

    let entry = commit_log
        .record_commit(
            tx_id,
            IsolationLevel::Serializable,
            affected_rows,
            param_hash,
        )
        .await
        .expect("commit");

    assert_eq!(entry.row_count_affected, affected_rows);
    assert_eq!(
        commit_log.get_affected_rows(tx_id),
        Some(affected_rows),
        "Affected rows preserved"
    );
}
