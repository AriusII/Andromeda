//! Commit log concurrency gates.

use super::fixtures::*;
use andromeda_transaction_log::{IsolationLevel, Lsn};
use andromeda_types::TransactionId;
use futures::future::join_all;
use std::collections::HashSet;
use std::sync::Arc;

#[tokio::test]
async fn test_concurrency_100_concurrent_commits() {
    let (_wal, _status_table, commit_log) = setup_commit_log();
    let commit_log = Arc::new(commit_log);

    let mut handles = vec![];

    for i in 0..100 {
        let commit_log = commit_log.clone();
        let handle = async move {
            let tx_id = TransactionId::new((i + 1) as u64);
            let entry = commit_log
                .record_commit(tx_id, IsolationLevel::Snapshot, 1, 0)
                .await
                .unwrap_or_else(|_| panic!("commit tx {}", i + 1));
            entry.commit_lsn.get()
        };
        handles.push(handle);
    }

    let mut lsns = vec![];
    for lsn in join_all(handles).await {
        lsns.push(lsn);
    }

    let unique_count = lsns.iter().collect::<HashSet<_>>().len();
    assert_eq!(
        unique_count, 100,
        "All 100 commits have unique LSNs (no duplicates)"
    );

    for lsn in &lsns {
        assert!(*lsn >= 1 && *lsn <= 100, "LSN {} in valid range", lsn);
    }

    for i in 1..=100 {
        let tx_id = TransactionId::new(i);
        assert!(
            commit_log.is_committed(tx_id),
            "Transaction {} is visible",
            i
        );
    }
}

#[tokio::test]
async fn test_concurrency_readers_see_ordered_commits() {
    let (_wal, _status_table, commit_log) = setup_commit_log();
    let commit_log = Arc::new(commit_log);

    let mut commit_lsns = vec![];
    for i in 1..=10 {
        let tx_id = TransactionId::new(i as u64);
        let entry = commit_log
            .record_commit(tx_id, IsolationLevel::Snapshot, i as u64, 0)
            .await
            .unwrap_or_else(|_| panic!("commit tx {}", i));
        commit_lsns.push((tx_id, entry.commit_lsn));
    }

    let mut reader_handles = vec![];
    for reader_id in 0..10 {
        let commit_log = commit_log.clone();
        let commit_lsns = commit_lsns.clone();

        let handle = async move {
            let mut prev_lsn = Lsn::new(0);
            for (tx_id, expected_lsn) in &commit_lsns {
                let lsn = commit_log.get_commit_lsn(*tx_id);
                assert_eq!(
                    lsn,
                    Some(*expected_lsn),
                    "Reader {} sees consistent LSN for tx",
                    reader_id
                );

                assert!(
                    expected_lsn.get() > prev_lsn.get(),
                    "Reader {} sees strictly ordered LSNs",
                    reader_id
                );
                prev_lsn = *expected_lsn;
            }
        };
        reader_handles.push(handle);
    }

    join_all(reader_handles).await;
}

#[tokio::test]
async fn test_concurrency_commits_and_readers_no_interference() {
    let (_wal, _status_table, commit_log) = setup_commit_log();
    let commit_log = Arc::new(commit_log);

    let mut writer_handles = vec![];

    for writer_id in 0..50 {
        let commit_log = commit_log.clone();
        let handle = async move {
            let tx_id = TransactionId::new((1000 + writer_id) as u64);
            let entry = commit_log
                .record_commit(tx_id, IsolationLevel::Snapshot, 1, 0)
                .await
                .unwrap_or_else(|_| panic!("writer {} commit", writer_id));
            (tx_id, entry.commit_lsn)
        };
        writer_handles.push(handle);
    }

    let mut reader_handles = vec![];

    for _reader_id in 0..50 {
        let commit_log = commit_log.clone();
        let handle = async move {
            for check_id in 0..100 {
                let tx_id = TransactionId::new((1000 + check_id % 50) as u64);
                let _is_committed = commit_log.is_committed(tx_id);
            }
        };
        reader_handles.push(handle);
    }

    join_all(writer_handles).await;
    join_all(reader_handles).await;
}
