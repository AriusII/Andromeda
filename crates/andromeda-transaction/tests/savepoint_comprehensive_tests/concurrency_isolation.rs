use super::*;

// TASK 5 — Concurrent & Isolation Tests

/// TC-CI-0001 · INV-SP-11
/// Two separate transactions have completely independent savepoint stacks.
#[test]
fn ci_savepoint_stacks_are_isolated_per_transaction() {
    let mgr = TransactionManager::new();
    let tx_a = mgr.begin().unwrap();
    let tx_b = mgr.begin().unwrap();

    mgr.create_savepoint(tx_a, "sp_a1").unwrap();
    mgr.create_savepoint(tx_a, "sp_a2").unwrap();
    mgr.create_savepoint(tx_b, "sp_b1").unwrap();

    assert_eq!(mgr.savepoint_depth(tx_a).unwrap(), 2);
    assert_eq!(mgr.savepoint_depth(tx_b).unwrap(), 1);

    // tx_b cannot access tx_a's savepoints.
    let err = mgr.rollback_to_savepoint(tx_b, "sp_a1").unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-CI-0002
/// Rolling back tx_a's savepoints does not affect tx_b's stack.
#[test]
fn ci_rollback_on_tx_a_does_not_affect_tx_b() {
    let mgr = TransactionManager::new();
    let tx_a = mgr.begin().unwrap();
    let tx_b = mgr.begin().unwrap();

    mgr.create_savepoint(tx_a, "a_root").unwrap();
    mgr.create_savepoint(tx_a, "a_work").unwrap();
    mgr.create_savepoint(tx_b, "b_root").unwrap();
    mgr.create_savepoint(tx_b, "b_work").unwrap();

    mgr.rollback_to_savepoint(tx_a, "a_root").unwrap();

    assert_eq!(
        mgr.savepoint_depth(tx_a).unwrap(),
        1,
        "tx_a rolled back to root"
    );
    assert_eq!(
        mgr.savepoint_depth(tx_b).unwrap(),
        2,
        "tx_b unaffected by tx_a rollback"
    );
}

/// TC-CI-0003
/// Committing tx_a does not affect tx_b's savepoints.
#[test]
fn ci_committing_tx_a_does_not_affect_tx_b_savepoints() {
    let mgr = TransactionManager::new();
    let tx_a = mgr.begin().unwrap();
    let tx_b = mgr.begin().unwrap();

    mgr.create_savepoint(tx_b, "b_checkpoint").unwrap();

    mgr.request_commit(tx_a).unwrap();
    mgr.commit_durable(tx_a, 1).unwrap();
    mgr.dispose(tx_a).unwrap();

    // tx_b savepoints must still be present.
    assert_eq!(mgr.savepoint_depth(tx_b).unwrap(), 1);
}

/// TC-CI-0004
/// Multiple transactions with the same savepoint name are independent.
#[test]
fn ci_same_savepoint_name_in_different_txs_is_allowed() {
    let mgr = TransactionManager::new();
    let tx_a = mgr.begin().unwrap();
    let tx_b = mgr.begin().unwrap();

    let sp_a = mgr.create_savepoint(tx_a, "checkpoint").unwrap();
    let sp_b = mgr.create_savepoint(tx_b, "checkpoint").unwrap();

    // Both succeed; ids are independent per-transaction allocators.
    assert_eq!(sp_a.id.get(), 1);
    assert_eq!(sp_b.id.get(), 1);
    assert_eq!(sp_a.rollback_marker.savepoint_id, sp_a.id);
    assert_eq!(sp_b.rollback_marker.savepoint_id, sp_b.id);
}

/// TC-CI-0005
/// Under RepeatableRead: tx sees its own savepoint-era rows but not concurrent
/// uncommitted writes from other transactions.
#[test]
fn ci_rr_savepoint_era_rows_visible_only_to_owner() {
    let status_table = TransactionStatusTable::new();
    let tx_owner = TransactionId::new(1);
    let tx_concurrent = TransactionId::new(2);
    status_table
        .record(tx_owner, TransactionStatus::InFlight)
        .unwrap();
    status_table
        .record(tx_concurrent, TransactionStatus::InFlight)
        .unwrap();

    let row_by_owner = MvccRowHeader {
        begin_ts: 10,
        end_ts: None,
        creator_tx_id: tx_owner,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    // Owner snapshot taken at ts=20.
    let owner_snap = snapshot_rr(tx_owner, 20, [tx_concurrent]);

    // Owner sees its own row.
    assert!(
        row_by_owner
            .visible_in_snapshot(&owner_snap, &status_table)
            .unwrap()
    );

    // Concurrent tx sees the owner's row as invisible (owner is in-flight,
    // not current transaction).
    let concurrent_snap = snapshot_rr(tx_concurrent, 20, [tx_owner]);
    assert!(
        !row_by_owner
            .visible_in_snapshot(&concurrent_snap, &status_table)
            .unwrap()
    );
}

/// TC-CI-0006
/// Snapshot isolation: a transaction's snapshot timestamp is fixed at creation;
/// rows written later are invisible even if committed before snapshot is read.
#[test]
fn ci_snapshot_ts_is_fixed_at_creation_future_commits_invisible() {
    let status_table = TransactionStatusTable::new();
    let late_writer = TransactionId::new(50);

    // late_writer commits at ts=30, but snapshot was taken at ts=20.
    status_table
        .record_committed_after_durable_wal(
            late_writer,
            andromeda_transaction_log::Lsn::new(1),
            andromeda_transaction_log::Lsn::new(1),
        )
        .unwrap();

    let late_row = MvccRowHeader {
        begin_ts: 25, // written after snapshot ts=20
        end_ts: None,
        creator_tx_id: late_writer,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    let snapshot = snapshot_rc(20); // snapshot at ts=20
    let visible = late_row
        .visible_in_snapshot(&snapshot, &status_table)
        .unwrap();
    assert!(
        !visible,
        "row created after snapshot ts must be invisible even if committed"
    );
}

/// TC-CI-0007
/// TransactionManager supports multiple concurrent transactions without
/// internal locking deadlocks (sequential simulation of concurrent workload).
#[test]
fn ci_concurrent_manager_operations_are_deadlock_free() {
    let mgr = TransactionManager::new();
    let txs: Vec<TransactionId> = (0..10).map(|_| mgr.begin().unwrap()).collect();

    // Simulate savepoint work on all transactions.
    for (i, &tx) in txs.iter().enumerate() {
        mgr.create_savepoint(tx, format!("sp_{i}")).unwrap();
    }

    // Rollback half.
    for &tx in &txs[..5] {
        let savepoint_name = format!("sp_{}", txs.iter().position(|t| t == &tx).unwrap());
        mgr.rollback_to_savepoint(tx, &savepoint_name).unwrap();
    }

    // Commit the other half.
    for &tx in &txs[5..] {
        mgr.request_commit(tx).unwrap();
        mgr.commit_durable(tx, tx.get()).unwrap();
        mgr.dispose(tx).unwrap();
    }

    // The 5 rolled-back txs still live.
    assert_eq!(mgr.live_count().unwrap(), 5);
}

/// TC-CI-0008
/// create_savepoint does not require or release any locks (2PL discipline).
#[test]
fn ci_create_savepoint_does_not_alter_lock_state() {
    use andromeda_locking::{LockManager, LockMode, LockResource};

    let mgr = TransactionManager::new();
    let locks = LockManager::new();
    let tx = mgr.begin().unwrap();

    let resource = LockResource::row(1, 1, 1).expect("valid row lock resource");
    // Acquire a lock.
    mgr.acquire_lock(&locks, tx, resource, LockMode::Exclusive)
        .unwrap();

    // Create a savepoint — locks must still be held.
    mgr.create_savepoint(tx, "mid_lock").unwrap();

    // The lock should still be held (no release).
    // Attempting to acquire the same lock in exclusive mode for a different tx
    // should see a conflict.
    let tx2 = mgr.begin().unwrap();
    let status = mgr
        .acquire_lock(&locks, tx2, resource, LockMode::Exclusive)
        .unwrap();
    // It's either waiting or conflicted — not immediately granted.
    use andromeda_locking::LockAcquireStatus;
    assert_ne!(
        status,
        LockAcquireStatus::Granted,
        "lock held by tx must conflict with exclusive acquisition by tx2"
    );
}

/// TC-CI-0009
/// Savepoint stack depth is preserved across manager snapshot queries.
#[test]
fn ci_savepoint_depth_query_is_consistent() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();

    for i in 0..5 {
        mgr.create_savepoint(tx, format!("p_{i}")).unwrap();
    }

    let record = mgr.snapshot(tx).unwrap().unwrap();
    assert_eq!(record.savepoint_depth, 5);
    assert_eq!(mgr.savepoint_depth(tx).unwrap(), 5);
}

/// TC-CI-0010
/// TransactionRecord snapshot is consistent: savepoint_depth matches
/// the actual stack after rollback.
#[test]
fn ci_transaction_record_snapshot_is_consistent_after_rollback() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();

    mgr.create_savepoint(tx, "s0").unwrap();
    mgr.create_savepoint(tx, "s1").unwrap();
    mgr.create_savepoint(tx, "s2").unwrap();

    mgr.rollback_to_savepoint(tx, "s0").unwrap();

    let record = mgr.snapshot(tx).unwrap().unwrap();
    assert_eq!(
        record.savepoint_depth, 1,
        "depth must reflect post-rollback state"
    );
}

/// TC-CI-0011
/// After releasing all savepoints, depth is zero and the manager snapshot
/// confirms this.
#[test]
fn ci_release_all_savepoints_yields_zero_depth() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();

    mgr.create_savepoint(tx, "alpha").unwrap();
    mgr.create_savepoint(tx, "beta").unwrap();
    mgr.release_savepoint(tx, "alpha").unwrap();

    let record = mgr.snapshot(tx).unwrap().unwrap();
    assert_eq!(record.savepoint_depth, 0);
}

/// TC-CI-0012
/// MVCC status table is thread-safe: concurrent status registrations for
/// disjoint tx_ids do not corrupt each other.
#[test]
fn ci_status_table_concurrent_registrations_are_correct() {
    use std::sync::Arc;
    use std::thread;

    let table = Arc::new(TransactionStatusTable::new());
    let handles: Vec<_> = (1_u64..=20)
        .map(|i| {
            let t = table.clone();
            thread::spawn(move || {
                let tx_id = TransactionId::new(i);
                t.record(tx_id, TransactionStatus::InFlight).unwrap();
                t.record_committed_after_durable_wal(
                    tx_id,
                    andromeda_transaction_log::Lsn::new(1),
                    andromeda_transaction_log::Lsn::new(1),
                )
                .unwrap();
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    for i in 1_u64..=20 {
        assert_eq!(
            table.status(TransactionId::new(i)),
            Some(TransactionStatus::Committed)
        );
    }
}

/// TC-CI-0013
/// Snapshot's is_transaction_active lookup is O(log n) via binary search.
/// We verify correctness for a large active set.
#[test]
fn ci_snapshot_active_tx_lookup_correct_for_large_set() {
    let active: Vec<TransactionId> = (1_u64..=100).map(TransactionId::new).collect();

    let snapshot = Snapshot::with_context(
        500,
        CatalogVersion::new(1),
        MvccIsolationPolicy::RepeatableRead,
        Some(TransactionId::new(200)),
        active.clone(),
    )
    .unwrap();

    for tx in &active {
        assert!(
            snapshot.is_transaction_active(*tx),
            "tx {} must be active in snapshot",
            tx.get()
        );
    }
    // A tx not in the set must not be active.
    assert!(!snapshot.is_transaction_active(TransactionId::new(999)));
}
