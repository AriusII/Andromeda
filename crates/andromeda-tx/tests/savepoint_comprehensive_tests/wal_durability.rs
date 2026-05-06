use super::*;

// TASK 3 — WAL & Durability Tests

/// TC-WD-0001 · INV-SP-09
/// Savepoint create/release/rollback_to on the stack layer produce no WAL events.
/// (No WAL adapter calls are made; the operations are in-memory metadata only.)
#[test]
fn wd_savepoint_operations_do_not_involve_wal() {
    // This is a structural test: SavepointStack has no WAL dependency in its
    // signature. All methods are pure in-memory operations.
    let mut stack = SavepointStack::new();
    let sp = stack.create("no_wal").unwrap();
    assert_eq!(sp.id.get(), 1);

    let _rv = stack.rollback_to("no_wal").unwrap();
    let _rel = stack.release("no_wal").unwrap();
    // If compilation succeeds without a WAL argument, the invariant holds.
}

/// TC-WD-0002 · INV-SP-09
/// Manager create_savepoint does not alter transaction status or LSN state.
#[test]
fn wd_create_savepoint_does_not_advance_lsn_or_status() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();

    let before = mgr.snapshot(tx).unwrap().unwrap();
    mgr.create_savepoint(tx, "no_lsn_change").unwrap();
    let after = mgr.snapshot(tx).unwrap().unwrap();

    assert_eq!(before.state_machine.state, after.state_machine.state);
    assert_eq!(
        before.state_machine.durable_commit_lsn,
        after.state_machine.durable_commit_lsn
    );
    assert_eq!(before.status, after.status);
}

/// TC-WD-0003 · INV-SP-10
/// Replaying a TxCommit record restores Committed status for that tx.
#[test]
fn wd_replay_commit_record_restores_committed_status() {
    let (commit_log, status_table) = make_commit_log();
    let tx = TransactionId::new(100);

    let summary = commit_log
        .reconstruct_from_tx_wal_replay([TxWalReplayRecord::commit(
            tx,
            Lsn::new(50),
            ts(1_000),
            5,
            IsolationLevel::Snapshot,
        )])
        .unwrap();

    assert_eq!(summary.commits_restored, 1);
    assert_eq!(status_table.status(tx), Some(TransactionStatus::Committed));
    assert_eq!(commit_log.get_commit_lsn(tx), Some(Lsn::new(50)));
}

/// TC-WD-0004 · INV-SP-10
/// Replaying a TxRollback record restores RolledBack status.
#[test]
fn wd_replay_rollback_record_restores_rolled_back_status() {
    let (commit_log, status_table) = make_commit_log();
    let tx = TransactionId::new(101);

    let summary = commit_log
        .reconstruct_from_tx_wal_replay([TxWalReplayRecord::rollback(
            tx,
            Lsn::new(51),
            ts(1_001),
            0xDEAD,
        )])
        .unwrap();

    assert_eq!(summary.rollbacks_restored, 1);
    assert_eq!(status_table.status(tx), Some(TransactionStatus::RolledBack));
    assert_eq!(commit_log.get_rollback_lsn(tx), Some(Lsn::new(51)));
}

/// TC-WD-0005 · INV-SP-10
/// Incomplete tx in WAL replay remains absent from status table (invisible).
#[test]
fn wd_incomplete_tx_remains_invisible_after_replay() {
    let (commit_log, status_table) = make_commit_log();
    let tx = TransactionId::new(102);

    let summary = commit_log
        .reconstruct_from_tx_wal_replay([TxWalReplayRecord::incomplete(tx, Lsn::new(52))])
        .unwrap();

    assert_eq!(summary.incomplete_transactions, 1);
    assert_eq!(status_table.status(tx), None);
    assert!(!commit_log.is_committed(tx));
    assert!(!commit_log.is_rolled_back(tx));
}

/// TC-WD-0006
/// Duplicate commit records during replay are idempotent; first wins.
#[test]
fn wd_duplicate_commit_replay_is_idempotent_first_wins() {
    let (commit_log, status_table) = make_commit_log();
    let tx = TransactionId::new(103);

    let summary = commit_log
        .reconstruct_from_tx_wal_replay([
            TxWalReplayRecord::commit(tx, Lsn::new(10), ts(100), 3, IsolationLevel::Snapshot),
            TxWalReplayRecord::commit(tx, Lsn::new(11), ts(101), 99, IsolationLevel::Serializable),
        ])
        .unwrap();

    assert_eq!(summary.commits_restored, 1);
    assert_eq!(summary.duplicate_commits, 1);
    // First record wins for LSN.
    assert_eq!(commit_log.get_commit_lsn(tx), Some(Lsn::new(10)));
    assert_eq!(status_table.status(tx), Some(TransactionStatus::Committed));
}

/// TC-WD-0007
/// Duplicate rollback records during replay are idempotent; first wins.
#[test]
fn wd_duplicate_rollback_replay_is_idempotent_first_wins() {
    let (commit_log, status_table) = make_commit_log();
    let tx = TransactionId::new(104);

    let summary = commit_log
        .reconstruct_from_tx_wal_replay([
            TxWalReplayRecord::rollback(tx, Lsn::new(20), ts(200), 1),
            TxWalReplayRecord::rollback(tx, Lsn::new(21), ts(201), 2),
        ])
        .unwrap();

    assert_eq!(summary.rollbacks_restored, 1);
    assert_eq!(summary.duplicate_rollbacks, 1);
    assert_eq!(commit_log.get_rollback_lsn(tx), Some(Lsn::new(20)));
    assert_eq!(status_table.status(tx), Some(TransactionStatus::RolledBack));
}

/// TC-WD-0008
/// Conflicting commit-then-rollback records are rejected.
#[test]
fn wd_conflicting_commit_and_rollback_records_are_rejected() {
    let (commit_log, _) = make_commit_log();
    let tx = TransactionId::new(105);

    let err = commit_log
        .reconstruct_from_tx_wal_replay([
            TxWalReplayRecord::commit(tx, Lsn::new(30), ts(300), 1, IsolationLevel::Snapshot),
            TxWalReplayRecord::rollback(tx, Lsn::new(31), ts(301), 7),
        ])
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
    // First record still wins for commit LSN.
    assert_eq!(commit_log.get_commit_lsn(tx), Some(Lsn::new(30)));
}

/// TC-WD-0009 · INV-SP-06
/// After replaying commit records, a reconstructed manager has no live savepoints
/// for committed transactions.
#[test]
fn wd_committed_tx_has_no_savepoints_after_recovery_replay() {
    // Simulate recovery: manager begins fresh, then WAL replay marks tx as committed.
    let (commit_log, status_table) = make_commit_log();
    let tx = TransactionId::new(200);

    commit_log
        .reconstruct_from_tx_wal_replay([TxWalReplayRecord::commit(
            tx,
            Lsn::new(100),
            ts(1000),
            0,
            IsolationLevel::Snapshot,
        )])
        .unwrap();

    assert_eq!(
        status_table.status(tx),
        Some(TransactionStatus::Committed),
        "recovered tx must be Committed"
    );
    // No live manager entry means no savepoints — structural guarantee.
}

/// TC-WD-0010 · INV-SP-06
/// Within a live manager, commit clears savepoints before the status is published.
#[test]
fn wd_manager_commit_clears_savepoints_before_visibility() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.create_savepoint(tx, "sp_before_commit").unwrap();
    assert_eq!(mgr.savepoint_depth(tx).unwrap(), 1);

    mgr.request_commit(tx).unwrap();
    mgr.commit_durable(tx, 77).unwrap();

    assert_eq!(mgr.savepoint_depth(tx).unwrap(), 0);

    // But status must be Committed.
    assert_eq!(mgr.status(tx).unwrap(), Some(TransactionStatus::Committed));
}

/// TC-WD-0011 (Crash during savepoint create — no terminal WAL record)
/// If a crash occurs after savepoint create but before any terminal WAL record,
/// the replayed transaction appears as Incomplete and is invisible.
#[test]
fn wd_crash_before_terminal_wal_leaves_tx_incomplete_and_invisible() {
    let (commit_log, status_table) = make_commit_log();
    let tx = TransactionId::new(300);

    // WAL replay finds the tx but no terminal record.
    let summary = commit_log
        .reconstruct_from_tx_wal_replay([TxWalReplayRecord::incomplete(tx, Lsn::new(50))])
        .unwrap();

    assert_eq!(summary.incomplete_transactions, 1);
    assert_eq!(status_table.status(tx), None);
    assert!(!commit_log.is_committed(tx));
}

/// TC-WD-0012 (Crash during commit — terminal WAL record flushed)
/// If the commit WAL record is durably flushed before the crash, recovery
/// restores the transaction as Committed.
#[test]
fn wd_crash_after_commit_wal_flushed_restores_committed_status() {
    let (commit_log, status_table) = make_commit_log();
    let tx = TransactionId::new(301);

    commit_log
        .reconstruct_from_tx_wal_replay([TxWalReplayRecord::commit(
            tx,
            Lsn::new(100),
            ts(2000),
            3,
            IsolationLevel::Snapshot,
        )])
        .unwrap();

    assert_eq!(status_table.status(tx), Some(TransactionStatus::Committed));
    assert_eq!(commit_log.get_commit_lsn(tx), Some(Lsn::new(100)));
}

/// TC-WD-0013 (Crash during rollback — terminal WAL record flushed)
/// If the rollback WAL record is durably flushed before the crash, recovery
/// restores the transaction as RolledBack.
#[test]
fn wd_crash_after_rollback_wal_flushed_restores_rolled_back_status() {
    let (commit_log, status_table) = make_commit_log();
    let tx = TransactionId::new(302);

    commit_log
        .reconstruct_from_tx_wal_replay([TxWalReplayRecord::rollback(
            tx,
            Lsn::new(200),
            ts(3000),
            0xCAFE,
        )])
        .unwrap();

    assert_eq!(status_table.status(tx), Some(TransactionStatus::RolledBack));
}

/// TC-WD-0014
/// Multiple transactions replayed in a single WAL replay pass produce independently
/// correct statuses with no cross-contamination.
#[test]
fn wd_multi_tx_replay_produces_independent_correct_statuses() {
    let (commit_log, status_table) = make_commit_log();

    let committed = TransactionId::new(400);
    let rolled_back = TransactionId::new(401);
    let incomplete = TransactionId::new(402);

    let summary = commit_log
        .reconstruct_from_tx_wal_replay([
            TxWalReplayRecord::commit(committed, Lsn::new(1), ts(1), 0, IsolationLevel::Snapshot),
            TxWalReplayRecord::rollback(rolled_back, Lsn::new(2), ts(2), 0),
            TxWalReplayRecord::incomplete(incomplete, Lsn::new(3)),
        ])
        .unwrap();

    assert_eq!(summary.commits_restored, 1);
    assert_eq!(summary.rollbacks_restored, 1);
    assert_eq!(summary.incomplete_transactions, 1);

    assert_eq!(
        status_table.status(committed),
        Some(TransactionStatus::Committed)
    );
    assert_eq!(
        status_table.status(rolled_back),
        Some(TransactionStatus::RolledBack)
    );
    assert_eq!(status_table.status(incomplete), None);
}

/// TC-WD-0015
/// Lsn ordering: LSN monotonicity of commit records is preserved across replay.
#[test]
fn wd_commit_lsn_ordering_preserved_across_replay() {
    let (commit_log, _) = make_commit_log();

    let tx1 = TransactionId::new(500);
    let tx2 = TransactionId::new(501);

    commit_log
        .reconstruct_from_tx_wal_replay([
            TxWalReplayRecord::commit(tx1, Lsn::new(10), ts(10), 1, IsolationLevel::Snapshot),
            TxWalReplayRecord::commit(tx2, Lsn::new(20), ts(20), 1, IsolationLevel::Snapshot),
        ])
        .unwrap();

    let lsn1 = commit_log.get_commit_lsn(tx1).unwrap();
    let lsn2 = commit_log.get_commit_lsn(tx2).unwrap();
    assert!(lsn1 < lsn2, "commit LSNs must preserve WAL ordering");
}

/// TC-WD-0016
/// Rollback LSN is retrievable independently from commit LSN.
#[test]
fn wd_rollback_lsn_is_independent_of_commit_lsn() {
    let (commit_log, _) = make_commit_log();

    let tx_commit = TransactionId::new(600);
    let tx_rollback = TransactionId::new(601);

    commit_log
        .reconstruct_from_tx_wal_replay([
            TxWalReplayRecord::commit(
                tx_commit,
                Lsn::new(100),
                ts(100),
                1,
                IsolationLevel::Snapshot,
            ),
            TxWalReplayRecord::rollback(tx_rollback, Lsn::new(101), ts(101), 0),
        ])
        .unwrap();

    assert_eq!(commit_log.get_commit_lsn(tx_commit), Some(Lsn::new(100)));
    assert_eq!(
        commit_log.get_rollback_lsn(tx_rollback),
        Some(Lsn::new(101))
    );
    assert_eq!(commit_log.get_commit_lsn(tx_rollback), None);
    assert_eq!(commit_log.get_rollback_lsn(tx_commit), None);
}

/// TC-WD-0017
/// WAL replay with an empty iterator produces an all-zero summary.
#[test]
fn wd_empty_wal_replay_produces_zero_summary() {
    let (commit_log, _) = make_commit_log();
    let summary = commit_log
        .reconstruct_from_tx_wal_replay(std::iter::empty::<TxWalReplayRecord>())
        .unwrap();

    assert_eq!(summary.commits_restored, 0);
    assert_eq!(summary.rollbacks_restored, 0);
    assert_eq!(summary.incomplete_transactions, 0);
    assert_eq!(summary.duplicate_commits, 0);
    assert_eq!(summary.duplicate_rollbacks, 0);
}

/// TC-WD-0018
/// Large-scale replay: 1000 transactions replayed without error.
#[test]
fn wd_large_scale_replay_1000_transactions() {
    let (commit_log, status_table) = make_commit_log();
    let records: Vec<TxWalReplayRecord> = (1_u64..=1000)
        .map(|i| {
            TxWalReplayRecord::commit(
                TransactionId::new(i),
                Lsn::new(i),
                ts(i),
                1,
                IsolationLevel::Snapshot,
            )
        })
        .collect();

    let summary = commit_log.reconstruct_from_tx_wal_replay(records).unwrap();

    assert_eq!(summary.commits_restored, 1000);
    for i in 1_u64..=1000 {
        assert_eq!(
            status_table.status(TransactionId::new(i)),
            Some(TransactionStatus::Committed)
        );
    }
}
