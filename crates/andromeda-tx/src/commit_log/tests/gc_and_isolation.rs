use super::super::*;
use super::support::*;

#[tokio::test]
async fn test_commit_log_gc_candidates() {
    let (_wal, _status_table, commit_log) = setup_commit_log();

    for i in 1..=5 {
        commit_log
            .record_commit(
                TransactionId::new(i as u64),
                IsolationLevel::Snapshot,
                i as u64,
                0,
            )
            .await
            .unwrap();
    }

    let candidates = commit_log.gc_candidates(Lsn::new(3));
    assert_eq!(candidates.len(), 2);
    assert!(candidates.contains(&TransactionId::new(1)));
    assert!(candidates.contains(&TransactionId::new(2)));
}

#[tokio::test]
async fn test_commit_log_verify_durability() {
    let (_wal, _status_table, commit_log) = setup_commit_log();

    let tx_id = TransactionId::new(1);
    commit_log
        .record_commit(tx_id, IsolationLevel::Snapshot, 5, 0)
        .await
        .unwrap();

    assert!(commit_log.verify_durability(tx_id).is_ok());
    assert!(
        commit_log
            .verify_durability(TransactionId::new(999))
            .is_err()
    );
}

#[tokio::test]
async fn test_commit_log_isolation_levels() {
    let (_wal, _status_table, commit_log) = setup_commit_log();

    let snapshot_entry = commit_log
        .record_commit(TransactionId::new(1), IsolationLevel::Snapshot, 10, 0)
        .await
        .unwrap();
    assert_eq!(snapshot_entry.isolation_level, IsolationLevel::Snapshot);

    let serializable_entry = commit_log
        .record_commit(TransactionId::new(2), IsolationLevel::Serializable, 20, 0)
        .await
        .unwrap();
    assert_eq!(
        serializable_entry.isolation_level,
        IsolationLevel::Serializable
    );
}
