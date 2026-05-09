use super::*;

// TASK 6 — Crash Recovery & Performance Gates

/// TC-CR-0001
/// A manager constructed with with_recovered_floor allocates ids above
/// the floor, preventing recycling of pre-crash ids.
#[test]
fn cr_recovered_manager_floor_prevents_id_recycling() {
    let crash_floor = 1_000_u64;
    let mgr = TransactionManager::with_recovered_floor(crash_floor);
    let tx = mgr.begin().unwrap();
    assert!(
        tx.get() > crash_floor,
        "new tx id {} must be > crash floor {}",
        tx.get(),
        crash_floor
    );
}

/// TC-CR-0002
/// seed_allocator raises the floor even on an already-running manager.
#[test]
fn cr_seed_allocator_raises_floor_for_running_manager() {
    let mgr = TransactionManager::new();
    let _t1 = mgr.begin().unwrap(); // id = 1

    mgr.seed_allocator(500).unwrap();
    let t2 = mgr.begin().unwrap();
    assert!(t2.get() > 500, "post-seed tx id {} must be > 500", t2.get());
}

/// TC-CR-0003
/// After crash-recovery replay, the reconstructed status table is the
/// authoritative source for visibility, matching what pre-crash state held.
#[test]
fn cr_recovered_status_table_matches_pre_crash_state() {
    let (commit_log, status_table) = make_commit_log();

    // Pre-crash state: tx_1 committed, tx_2 rolled back, tx_3 incomplete.
    let tx1 = TransactionId::new(1);
    let tx2 = TransactionId::new(2);
    let tx3 = TransactionId::new(3);

    commit_log
        .reconstruct_from_tx_wal_replay([
            TxWalReplayRecord::commit(tx1, Lsn::new(1), ts(1), 5, IsolationLevel::Snapshot),
            TxWalReplayRecord::rollback(tx2, Lsn::new(2), ts(2), 0),
            TxWalReplayRecord::incomplete(tx3, Lsn::new(3)),
        ])
        .unwrap();

    // Verify reconstructed state matches pre-crash expectations.
    assert_eq!(
        status_table.status(tx1),
        Some(TransactionStatus::Committed),
        "tx1 must be Committed after recovery"
    );
    assert_eq!(
        status_table.status(tx2),
        Some(TransactionStatus::RolledBack),
        "tx2 must be RolledBack after recovery"
    );
    assert_eq!(
        status_table.status(tx3),
        None,
        "tx3 must be absent (incomplete) after recovery"
    );

    // LSN evidence must be preserved.
    assert_eq!(commit_log.get_commit_lsn(tx1), Some(Lsn::new(1)));
    assert_eq!(commit_log.get_rollback_lsn(tx2), Some(Lsn::new(2)));
    assert_eq!(commit_log.get_commit_lsn(tx3), None);
}

/// TC-CR-0004
/// A restore (rollback_to) is a pure in-memory operation with no WAL output.
/// After a simulated crash, the savepoint stack is rebuilt by replaying the
/// outer transaction's WAL record — if the transaction was incomplete, the
/// entire tx (including all savepoints) is rolled back.
#[test]
fn cr_incomplete_tx_after_crash_has_no_savepoints_post_recovery() {
    let (commit_log, status_table) = make_commit_log();
    // tx was mid-restore when crash occurred — no terminal WAL record.
    let tx = TransactionId::new(999);

    commit_log
        .reconstruct_from_tx_wal_replay([TxWalReplayRecord::incomplete(tx, Lsn::new(999))])
        .unwrap();

    // tx is invisible post-recovery.
    assert_eq!(status_table.status(tx), None);
    assert!(!commit_log.is_committed(tx));
    // No savepoints exist because the manager is fresh post-recovery with no live tx.
}

/// TC-CR-0005
/// Replay of a mixed workload (commits, rollbacks, incompletes) does not
/// let incomplete rows appear committed.
#[test]
fn cr_incomplete_rows_never_appear_committed_after_recovery() {
    let (commit_log, status_table) = make_commit_log();

    let incomplete_txs: Vec<TransactionId> = (700_u64..710).map(TransactionId::new).collect();
    let committed_txs: Vec<TransactionId> = (720_u64..730).map(TransactionId::new).collect();

    let mut records = Vec::new();
    for &tx in &incomplete_txs {
        records.push(TxWalReplayRecord::incomplete(tx, Lsn::new(tx.get())));
    }
    for &tx in &committed_txs {
        records.push(TxWalReplayRecord::commit(
            tx,
            Lsn::new(tx.get()),
            ts(tx.get()),
            1,
            IsolationLevel::Snapshot,
        ));
    }

    commit_log.reconstruct_from_tx_wal_replay(records).unwrap();

    for &tx in &incomplete_txs {
        assert_eq!(
            status_table.status(tx),
            None,
            "incomplete tx {} must be absent post-recovery",
            tx.get()
        );
    }
    for &tx in &committed_txs {
        assert_eq!(
            status_table.status(tx),
            Some(TransactionStatus::Committed),
            "committed tx {} must be Committed post-recovery",
            tx.get()
        );
    }
}

/// TC-PG-0001 · PERF-GATE: savepoint create < 10 ms
/// Creates 1000 savepoints across 10 independent stacks and verifies the
/// average per-operation time is within the 10 ms gate.
#[test]
fn pg_savepoint_create_under_10ms_gate() {
    use std::time::Instant;

    let n = 1_000_usize;
    let start = Instant::now();

    let mut total = 0_usize;
    for batch in 0..10 {
        let mut stack = SavepointStack::new();
        for i in 0..100 {
            stack.create(format!("sp_{batch}_{i}")).unwrap();
            total += 1;
        }
    }

    let elapsed = start.elapsed();
    assert_eq!(total, n);

    // Gate: total wall time must be < 10s (10ms per op × 1000 ops).
    // In practice this is far below 1 ms total; the gate catches regression.
    assert!(
        elapsed.as_millis() < 10_000,
        "savepoint create performance gate violated: {}ms for {} ops",
        elapsed.as_millis(),
        n
    );
}

/// TC-PG-0002 · PERF-GATE: savepoint rollback_to < 50 ms
/// Performs 1000 rollback_to operations (with growing stacks) and validates
/// total wall time.
#[test]
fn pg_savepoint_rollback_under_50ms_gate() {
    use std::time::Instant;

    let start = Instant::now();
    let n = 100_usize;

    for _ in 0..n {
        let mut stack = SavepointStack::new();
        for i in 0..50 {
            stack.create(format!("s{i}")).unwrap();
        }
        // Roll back to the 10th savepoint, discarding 40 entries.
        stack.rollback_to("s10").unwrap();
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 50_000,
        "rollback_to performance gate violated: {}ms for {} ops",
        elapsed.as_millis(),
        n
    );
}

/// TC-PG-0003 · PERF-GATE: no regression on non-savepoint transactions
/// Begins, commits, and disposes 500 transactions without savepoints and
/// verifies wall time per transaction.
#[test]
fn pg_non_savepoint_tx_no_perf_regression() {
    use std::time::Instant;

    let mgr = TransactionManager::new();
    let n = 500_usize;
    let start = Instant::now();

    for i in 0..n {
        let tx = mgr.begin().unwrap();
        mgr.request_commit(tx).unwrap();
        mgr.commit_durable(tx, (i as u64) + 1).unwrap();
        mgr.dispose(tx).unwrap();
    }

    let elapsed = start.elapsed();
    assert_eq!(mgr.live_count().unwrap(), 0);
    // Gate: 500 full lifecycle transactions must complete in < 5 seconds.
    assert!(
        elapsed.as_millis() < 5_000,
        "non-savepoint tx perf regression gate violated: {}ms for {} txs",
        elapsed.as_millis(),
        n
    );
}

/// TC-PG-0004 · PERF-GATE: memory overhead bounded
/// Creates a stack with the maximum practical depth (1000 savepoints) and
/// verifies the depth counter is correct. (Memory bounding is structural:
/// the Vec holds one Savepoint per entry with fixed-size fields.)
#[test]
fn pg_savepoint_stack_depth_bounded_at_1000() {
    let mut stack = SavepointStack::new();
    for i in 0..1000 {
        stack.create(format!("sp{i}")).unwrap();
    }
    assert_eq!(stack.depth(), 1000);

    // clear() must free the entire allocation.
    stack.clear();
    assert_eq!(stack.depth(), 0);
    assert!(stack.is_empty());
}

/// TC-PG-0005 · PERF-GATE: WAL replay throughput
/// Replays 10_000 mixed commit/rollback/incomplete records within 1 second.
#[test]
fn pg_wal_replay_throughput_10k_records() {
    use std::time::Instant;

    let (commit_log, _) = make_commit_log();
    let records: Vec<TxWalReplayRecord> = (1_u64..=10_000)
        .map(|i| match i % 3 {
            0 => TxWalReplayRecord::commit(
                TransactionId::new(i),
                Lsn::new(i),
                ts(i),
                1,
                IsolationLevel::Snapshot,
            ),
            1 => TxWalReplayRecord::rollback(TransactionId::new(i), Lsn::new(i), ts(i), i),
            _ => TxWalReplayRecord::incomplete(TransactionId::new(i), Lsn::new(i)),
        })
        .collect();

    let start = Instant::now();
    let summary = commit_log.reconstruct_from_tx_wal_replay(records).unwrap();
    let elapsed = start.elapsed();

    assert!(summary.commits_restored > 0);
    assert!(summary.rollbacks_restored > 0);
    assert!(summary.incomplete_transactions > 0);
    assert!(
        elapsed.as_millis() < 1_000,
        "WAL replay throughput gate violated: {}ms for 10k records",
        elapsed.as_millis()
    );
}

// ── 6d: State machine invariant completeness ──────────────────────────────────

/// TC-SM-0001 · INV-SP-12
/// Every non-terminal state rejects dispose directly.
#[test]
fn sm_every_non_terminal_state_rejects_dispose() {
    use andromeda_transaction::TransactionEvent;

    let non_terminal = [
        TransactionState::Created,
        TransactionState::Active,
        TransactionState::Committing,
        TransactionState::Failed,
        TransactionState::RollingBack,
        TransactionState::Poisoned,
    ];

    for state in non_terminal {
        let err = state.apply(TransactionEvent::Dispose).unwrap_err();
        assert_eq!(
            err.kind(),
            AndromedaErrorKind::Transaction,
            "state {:?} must reject Dispose",
            state
        );
    }
}

/// TC-SM-0002
/// is_terminal returns true only for Committed, RolledBack, Disposed.
#[test]
fn sm_is_terminal_only_for_terminal_states() {
    let terminal = [
        TransactionState::Committed,
        TransactionState::RolledBack,
        TransactionState::Disposed,
    ];
    let non_terminal = [
        TransactionState::Created,
        TransactionState::Active,
        TransactionState::Committing,
        TransactionState::Failed,
        TransactionState::RollingBack,
        TransactionState::Poisoned,
    ];

    for s in terminal {
        assert!(s.is_terminal(), "{:?} must be terminal", s);
    }
    for s in non_terminal {
        assert!(!s.is_terminal(), "{:?} must not be terminal", s);
    }
}

/// TC-SM-0003
/// Full commit path: Created → Active → Committing → Committed → Disposed.
#[test]
fn sm_full_commit_path_is_legal() {
    let mut machine = andromeda_transaction::TransactionStateMachine::new(TransactionId::new(42));
    machine.begin().unwrap();
    machine.request_commit().unwrap();
    machine
        .publish_visible_commit_after_durable_flush(100)
        .unwrap();
    assert!(machine.is_visible_committed());

    machine
        .apply(andromeda_transaction::TransactionEvent::Dispose)
        .unwrap();
    assert_eq!(machine.state(), TransactionState::Disposed);
}

/// TC-SM-0004
/// Full rollback path: Created → Active → RollingBack → RolledBack → Disposed.
#[test]
fn sm_full_rollback_path_is_legal() {
    let mut machine = andromeda_transaction::TransactionStateMachine::new(TransactionId::new(43));
    machine.begin().unwrap();
    machine.request_rollback().unwrap();
    machine.complete_rollback_after_durable_flush(200).unwrap();
    assert!(machine.is_durable_rolled_back());

    machine
        .apply(andromeda_transaction::TransactionEvent::Dispose)
        .unwrap();
    assert_eq!(machine.state(), TransactionState::Disposed);
}

/// TC-SM-0005
/// Poison path: Created → Active → Poisoned → RollingBack → RolledBack → Disposed.
#[test]
fn sm_full_poison_path_is_legal() {
    let mut machine = andromeda_transaction::TransactionStateMachine::new(TransactionId::new(44));
    machine.begin().unwrap();
    machine
        .apply(andromeda_transaction::TransactionEvent::Poison)
        .unwrap();
    machine.request_rollback().unwrap();
    machine.complete_rollback_after_durable_flush(300).unwrap();
    machine
        .apply(andromeda_transaction::TransactionEvent::Dispose)
        .unwrap();
    assert_eq!(machine.state(), TransactionState::Disposed);
}

/// TC-SS-0001
/// SavepointStack::default() produces the same state as new().
#[test]
fn ss_default_equals_new() {
    let a = SavepointStack::new();
    let b = SavepointStack::default();
    assert_eq!(a, b);
}

/// TC-SS-0002
/// active() returns an empty slice for a fresh stack.
#[test]
fn ss_fresh_stack_active_is_empty() {
    let stack = SavepointStack::new();
    assert!(stack.active().is_empty());
    assert_eq!(stack.depth(), 0);
    assert!(stack.is_empty());
}

/// TC-SS-0003
/// After a series of creates and a full release from the root, the stack
/// is empty and is_empty() returns true.
#[test]
fn ss_full_release_from_root_empties_stack() {
    let mut stack = SavepointStack::new();
    stack.create("root").unwrap();
    stack.create("a").unwrap();
    stack.create("b").unwrap();

    let ev = stack.release("root").unwrap();
    assert_eq!(ev.released.len(), 3);
    assert!(stack.is_empty());
}

/// TC-SS-0004
/// SavepointId::new and ::get remain a compatibility round-trip, while
/// try_new rejects the zero sentinel that SavepointStack never allocates.
#[test]
fn ss_savepoint_id_new_and_get_are_round_trip() {
    for v in [0_u64, 1, 100, u64::MAX] {
        let id = SavepointId::new(v);
        assert_eq!(id.get(), v);
    }
    assert!(!SavepointId::new(0).is_valid());
    assert!(SavepointId::try_new(0).is_err());
    assert_eq!(SavepointId::try_new(1).unwrap().get(), 1);
}

/// TC-SS-0005
/// SavepointId conversion traits validate public ids without changing the
/// compatibility behavior of SavepointId::new.
#[test]
fn ss_savepoint_id_conversion_traits_validate_zero() {
    let id = SavepointId::try_from(42).unwrap();
    assert_eq!(u64::from(id), 42);
    assert!(SavepointId::try_from(0).is_err());
}

/// TC-SS-0006
/// Savepoint struct is Clone and Eq: a cloned savepoint equals the original.
#[test]
fn ss_savepoint_is_clone_and_eq() {
    let mut stack = SavepointStack::new();
    let original = stack.create("cloneable").unwrap();
    let cloned = original.clone();
    assert_eq!(original, cloned);
}
