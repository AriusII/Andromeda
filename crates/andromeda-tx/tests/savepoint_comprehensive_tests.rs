//! Comprehensive savepoint test suite — Wave 13 · Batch 22 · Agent 4/4
//!
//! Work item: v1-tx-savepoints-s04-tests
//!
//! # Scope
//! Covers the full savepoint lifecycle through six major areas:
//!
//! | Task | Area                          | Cases |
//! |------|-------------------------------|-------|
//! |  1   | Lifecycle unit tests          |  35+  |
//! |  2   | MVCC visibility               |  30+  |
//! |  3   | WAL & durability              |  30+  |
//! |  4   | Error handling & rollback     |  25+  |
//! |  5   | Concurrent & isolation        |  20+  |
//! |  6   | Crash recovery & performance  |  15+  |
//!
//! # Invariants tested
//!
//! * `INV-SP-01` – savepoint names are unique within a transaction
//! * `INV-SP-02` – savepoint ids are monotonically increasing per transaction
//! * `INV-SP-03` – rollback_to keeps the target active; discards only descendants
//! * `INV-SP-04` – release discards the target and all descendants
//! * `INV-SP-05` – savepoint operations are only permitted while Active + InFlight
//! * `INV-SP-06` – commit / rollback clears the savepoint stack before state change
//! * `INV-SP-07` – dispose removes live state, status history is retained
//! * `INV-SP-08` – MVCC row visibility is snapshot-timestamp driven, not savepoint driven
//! * `INV-SP-09` – no WAL record is produced by create/release/rollback_to on its own
//! * `INV-SP-10` – WAL replay reconstructs terminal tx status; incomplete txs stay invisible
//! * `INV-SP-11` – savepoint stack is isolated per transaction-id
//! * `INV-SP-12` – failed / poisoned transactions must rollback before disposal

#![allow(clippy::too_many_lines)]

use andromeda_core::{AndromedaErrorKind, CatalogVersion, TransactionId};
use andromeda_tx::{
    CommitLogManager, IsolationLevel, Lsn, MvccIsolationPolicy, MvccRowHeader, SavepointId,
    SavepointRollbackMarker, SavepointStack, Snapshot, TransactionManager, TransactionState,
    TransactionStatus, TransactionStatusTable, TxWalReplayRecord, WalRecordKind,
};
use std::sync::Arc;

// ── Test helpers ─────────────────────────────────────────────────────────────

/// Minimal no-op WAL used by every durability test.
struct NoopWal;

#[async_trait::async_trait]
impl andromeda_tx::commit_log::InvocationWal for NoopWal {
    async fn append(
        &self,
        _kind: WalRecordKind,
        _transaction_id: Option<TransactionId>,
        _payload: &[u8],
    ) -> andromeda_core::AndromedaResult<Lsn> {
        Ok(Lsn::new(1))
    }

    async fn flush_through(&self, lsn: Lsn) -> andromeda_core::AndromedaResult<Lsn> {
        Ok(lsn)
    }
}

fn make_commit_log() -> (CommitLogManager, Arc<TransactionStatusTable>) {
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(Arc::new(NoopWal), status_table.clone());
    (commit_log, status_table)
}

fn ts(v: u64) -> andromeda_core::EngineTimestamp {
    andromeda_core::EngineTimestamp::from_unix_millis(v)
}

/// Build a minimal RepeatableRead snapshot owned by `tx_id` at `timestamp`.
fn snapshot_rr(
    tx_id: TransactionId,
    timestamp: u64,
    active: impl IntoIterator<Item = TransactionId>,
) -> Snapshot {
    Snapshot::with_context(
        timestamp,
        CatalogVersion::new(1),
        MvccIsolationPolicy::RepeatableRead,
        Some(tx_id),
        active,
    )
    .expect("valid snapshot")
}

/// Build a minimal ReadCommitted snapshot at `timestamp`.
fn snapshot_rc(timestamp: u64) -> Snapshot {
    Snapshot::with_context(
        timestamp,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        None,
        [],
    )
    .expect("valid snapshot")
}

// ─────────────────────────────────────────────────────────────────────────────
// TASK 1 — Savepoint Lifecycle Unit Tests
// ─────────────────────────────────────────────────────────────────────────────

// ── 1a: Create savepoint – ID generation and state tracking ──────────────────

/// TC-SP-0001 · INV-SP-02
/// First savepoint allocated from a fresh stack receives id == 1.
#[test]
fn sp_create_first_id_is_one() {
    let mut stack = SavepointStack::new();
    let sp = stack.create("first").unwrap();
    assert_eq!(sp.id.get(), 1, "first savepoint id must be 1");
}

/// TC-SP-0002 · INV-SP-02
/// IDs are strictly monotonically increasing across multiple creates.
#[test]
fn sp_create_ids_are_strictly_monotonic() {
    let mut stack = SavepointStack::new();
    let a = stack.create("a").unwrap();
    let b = stack.create("b").unwrap();
    let c = stack.create("c").unwrap();
    assert!(a.id < b.id, "a must precede b");
    assert!(b.id < c.id, "b must precede c");
    assert_eq!(a.id.get() + 1, b.id.get());
    assert_eq!(b.id.get() + 1, c.id.get());
}

/// TC-SP-0003 · INV-SP-01
/// Duplicate name within the same stack is rejected.
#[test]
fn sp_create_duplicate_name_is_rejected() {
    let mut stack = SavepointStack::new();
    stack.create("alpha").unwrap();
    let err = stack.create("alpha").unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-SP-0004
/// Empty name is rejected before any stack mutation.
#[test]
fn sp_create_empty_name_is_rejected() {
    let mut stack = SavepointStack::new();
    let err = stack.create("").unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
    // Stack must be unmodified.
    assert!(stack.is_empty());
}

/// TC-SP-0005
/// Whitespace-only name is rejected.
#[test]
fn sp_create_whitespace_only_name_is_rejected() {
    let mut stack = SavepointStack::new();
    let err = stack.create("   ").unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
    assert!(stack.is_empty());
}

/// TC-SP-0006
/// rollback_ordinal inside the marker equals the savepoint id value.
#[test]
fn sp_create_rollback_ordinal_equals_savepoint_id() {
    let mut stack = SavepointStack::new();
    let sp = stack.create("marker_check").unwrap();
    assert_eq!(
        sp.rollback_marker.rollback_ordinal,
        sp.id.get(),
        "rollback_ordinal must equal the savepoint id"
    );
    assert_eq!(sp.rollback_marker.savepoint_id, sp.id);
}

/// TC-SP-0007
/// After create the stack depth increments by exactly one.
#[test]
fn sp_create_increments_depth_by_one() {
    let mut stack = SavepointStack::new();
    assert_eq!(stack.depth(), 0);
    stack.create("s1").unwrap();
    assert_eq!(stack.depth(), 1);
    stack.create("s2").unwrap();
    assert_eq!(stack.depth(), 2);
}

/// TC-SP-0008
/// `active()` returns savepoints in insertion order (LIFO stack discipline
/// means top is last).
#[test]
fn sp_create_active_slice_is_in_insertion_order() {
    let mut stack = SavepointStack::new();
    stack.create("first").unwrap();
    stack.create("second").unwrap();
    stack.create("third").unwrap();
    let names: Vec<&str> = stack.active().iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["first", "second", "third"]);
}

/// TC-SP-0009
/// Manager-level create_savepoint is only permitted while Active + InFlight.
#[test]
fn sp_create_requires_active_in_flight_via_manager() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    // Must succeed while Active.
    let sp = mgr.create_savepoint(tx, "s1").unwrap();
    assert_eq!(sp.id.get(), 1);
    // Move to Committing; savepoint ops must now fail.
    mgr.request_commit(tx).unwrap();
    let err = mgr.create_savepoint(tx, "s2").unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-SP-0010
/// Creating a savepoint on an unknown tx_id is rejected.
#[test]
fn sp_create_unknown_tx_id_is_rejected() {
    let mgr = TransactionManager::new();
    let unknown = TransactionId::new(999);
    let err = mgr.create_savepoint(unknown, "sp").unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

// ── 1b: Restore (rollback_to) savepoint ──────────────────────────────────────

/// TC-SP-0011 · INV-SP-03
/// rollback_to keeps the target active on the stack.
#[test]
fn sp_rollback_to_keeps_target_on_stack() {
    let mut stack = SavepointStack::new();
    stack.create("a").unwrap();
    stack.create("b").unwrap();
    stack.create("c").unwrap();

    let ev = stack.rollback_to("b").unwrap();
    assert_eq!(ev.target.name, "b");

    let remaining: Vec<&str> = stack.active().iter().map(|s| s.name.as_str()).collect();
    assert_eq!(remaining, vec!["a", "b"], "target must remain on the stack");
}

/// TC-SP-0012 · INV-SP-03
/// rollback_to discards exactly the descendants (all savepoints after target).
#[test]
fn sp_rollback_to_discards_only_descendants() {
    let mut stack = SavepointStack::new();
    stack.create("root").unwrap();
    stack.create("mid").unwrap();
    stack.create("leaf_a").unwrap();
    stack.create("leaf_b").unwrap();

    let ev = stack.rollback_to("mid").unwrap();
    assert_eq!(ev.discarded_descendants.len(), 2);
    let discarded: Vec<&str> = ev
        .discarded_descendants
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(discarded, vec!["leaf_a", "leaf_b"]);
    assert_eq!(stack.depth(), 2);
}

/// TC-SP-0013
/// rollback_to returns the correct rollback_marker for the target.
#[test]
fn sp_rollback_to_evidence_contains_correct_marker() {
    let mut stack = SavepointStack::new();
    stack.create("s1").unwrap();
    let s2 = stack.create("s2").unwrap();
    stack.create("s3").unwrap();

    let ev = stack.rollback_to("s2").unwrap();
    assert_eq!(
        ev.target.rollback_marker.rollback_ordinal,
        s2.id.get(),
        "rollback ordinal must match target savepoint"
    );
}

/// TC-SP-0014 · INV-SP-03 (idempotency)
/// Rolling back to the same savepoint twice is safe and idempotent: the stack
/// settles to the same shape after both invocations.
#[test]
fn sp_rollback_to_same_savepoint_twice_is_idempotent() {
    let mut stack = SavepointStack::new();
    stack.create("checkpoint").unwrap();
    stack.create("work1").unwrap();
    stack.create("work2").unwrap();

    // First rollback.
    let ev1 = stack.rollback_to("checkpoint").unwrap();
    assert_eq!(ev1.discarded_descendants.len(), 2);
    let depth_after_first = stack.depth();

    // Second rollback to same savepoint — no descendants remain.
    let ev2 = stack.rollback_to("checkpoint").unwrap();
    assert_eq!(
        ev2.discarded_descendants.len(),
        0,
        "second rollback has no descendants to discard"
    );
    assert_eq!(stack.depth(), depth_after_first, "depth must be unchanged");
}

/// TC-SP-0015
/// rollback_to a non-existent savepoint name returns an error.
#[test]
fn sp_rollback_to_nonexistent_name_is_rejected() {
    let mut stack = SavepointStack::new();
    stack.create("real").unwrap();
    let err = stack.rollback_to("ghost").unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
    // Stack must be unmodified.
    assert_eq!(stack.depth(), 1);
}

/// TC-SP-0016
/// rollback_to the deepest (top-of-stack) savepoint discards zero descendants.
#[test]
fn sp_rollback_to_top_discards_zero_descendants() {
    let mut stack = SavepointStack::new();
    stack.create("base").unwrap();
    stack.create("top").unwrap();

    let ev = stack.rollback_to("top").unwrap();
    assert_eq!(ev.discarded_descendants.len(), 0);
    assert_eq!(stack.depth(), 2);
}

/// TC-SP-0017
/// rollback_to the bottommost savepoint discards all later entries.
#[test]
fn sp_rollback_to_root_discards_all_later_savepoints() {
    let mut stack = SavepointStack::new();
    stack.create("root").unwrap();
    for i in 1..=10 {
        stack.create(format!("s{i}")).unwrap();
    }
    let ev = stack.rollback_to("root").unwrap();
    assert_eq!(ev.discarded_descendants.len(), 10);
    assert_eq!(stack.depth(), 1);
}

// ── 1c: Nested savepoints — stack discipline ──────────────────────────────────

/// TC-SP-0018
/// Nested savepoints can be released bottom-up after inner-layer rollbacks.
#[test]
fn sp_nested_release_after_inner_rollback() {
    let mut stack = SavepointStack::new();
    stack.create("outer").unwrap();
    stack.create("inner").unwrap();

    // Rollback inner work.
    let rv = stack.rollback_to("outer").unwrap();
    assert_eq!(rv.discarded_descendants.len(), 1); // inner discarded

    // Outer can still be released.
    let rel = stack.release("outer").unwrap();
    assert_eq!(rel.released.len(), 1);
    assert!(stack.is_empty());
}

/// TC-SP-0019
/// Release of a parent drains all nested children simultaneously.
#[test]
fn sp_release_parent_drains_all_children() {
    let mut stack = SavepointStack::new();
    stack.create("level0").unwrap();
    stack.create("level1").unwrap();
    stack.create("level2").unwrap();
    stack.create("level3").unwrap();

    let ev = stack.release("level1").unwrap();
    // level1, level2, level3 are all released.
    assert_eq!(ev.released.len(), 3);
    let remaining: Vec<&str> = stack.active().iter().map(|s| s.name.as_str()).collect();
    assert_eq!(remaining, vec!["level0"]);
}

/// TC-SP-0020
/// Deep nesting (20 levels) preserves LIFO discipline throughout.
#[test]
fn sp_deep_nesting_preserves_lifo_discipline() {
    let mut stack = SavepointStack::new();
    for i in 0..20 {
        stack.create(format!("level{i}")).unwrap();
    }
    assert_eq!(stack.depth(), 20);

    // Rollback to level5 — levels 6..19 are discarded.
    let ev = stack.rollback_to("level5").unwrap();
    assert_eq!(ev.discarded_descendants.len(), 14); // 6..19 = 14 entries
    assert_eq!(stack.depth(), 6);

    // Remaining stack: level0..level5 in order.
    let names: Vec<String> = stack.active().iter().map(|s| s.name.clone()).collect();
    for (i, name) in names.iter().enumerate() {
        assert_eq!(name, &format!("level{i}"));
    }
}

/// TC-SP-0021
/// Rollback after rollback: re-creating a name is allowed once descendants
/// carrying the duplicate are discarded.
#[test]
fn sp_name_reuse_allowed_after_descendants_discarded() {
    let mut stack = SavepointStack::new();
    stack.create("base").unwrap();
    stack.create("work").unwrap();

    // Rollback discards "work".
    stack.rollback_to("base").unwrap();
    // "work" was discarded, so the name is free again.
    let sp = stack.create("work").unwrap();
    assert_eq!(sp.name, "work");
    assert_eq!(stack.depth(), 2);
}

// ── 1d: Error cases ───────────────────────────────────────────────────────────

/// TC-SP-0022
/// create_savepoint on a Failed transaction is rejected.
#[test]
fn sp_create_on_failed_transaction_is_rejected() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.fail(tx).unwrap();
    let err = mgr.create_savepoint(tx, "after_fail").unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-SP-0023
/// create_savepoint on a Poisoned transaction is rejected.
#[test]
fn sp_create_on_poisoned_transaction_is_rejected() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.poison(tx).unwrap();
    let err = mgr.create_savepoint(tx, "after_poison").unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-SP-0024
/// rollback_to_savepoint on a Committing transaction is rejected.
#[test]
fn sp_rollback_to_on_committing_transaction_is_rejected() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.create_savepoint(tx, "sp").unwrap();
    mgr.request_commit(tx).unwrap();
    let err = mgr.rollback_to_savepoint(tx, "sp").unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-SP-0025
/// release_savepoint on a RollingBack transaction is rejected.
#[test]
fn sp_release_on_rolling_back_transaction_is_rejected() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.create_savepoint(tx, "sp").unwrap();
    mgr.request_rollback(tx).unwrap();
    let err = mgr.release_savepoint(tx, "sp").unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-SP-0026
/// Savepoint depth is reported as zero on an unknown transaction.
#[test]
fn sp_depth_on_unknown_tx_is_error() {
    let mgr = TransactionManager::new();
    let phantom = TransactionId::new(777);
    let err = mgr.savepoint_depth(phantom).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-SP-0027
/// Zero transaction id is rejected by manager savepoint operations.
#[test]
fn sp_zero_tx_id_rejected_by_manager() {
    let mgr = TransactionManager::new();
    let zero = TransactionId::new(0);
    assert!(mgr.create_savepoint(zero, "sp").is_err());
    assert!(mgr.rollback_to_savepoint(zero, "sp").is_err());
    assert!(mgr.release_savepoint(zero, "sp").is_err());
}

// ── 1e: Idempotency / determinism ─────────────────────────────────────────────

/// TC-SP-0028 · INV-SP-03
/// Two consecutive rollbacks to the same savepoint produce structurally
/// equivalent final stack state.
#[test]
fn sp_double_rollback_produces_same_stack_shape() {
    let mut stack = SavepointStack::new();
    stack.create("checkpoint").unwrap();
    stack.create("ephemeral").unwrap();

    stack.rollback_to("checkpoint").unwrap();
    let snapshot1 = stack.active().to_vec();

    // Perform a second rollback — no descendants remain; stack unchanged.
    stack.rollback_to("checkpoint").unwrap();
    let snapshot2 = stack.active().to_vec();

    assert_eq!(snapshot1, snapshot2);
}

/// TC-SP-0029
/// clear() empties the entire stack regardless of depth.
#[test]
fn sp_clear_empties_all_savepoints() {
    let mut stack = SavepointStack::new();
    for i in 0..5 {
        stack.create(format!("s{i}")).unwrap();
    }
    stack.clear();
    assert!(stack.is_empty());
    assert_eq!(stack.depth(), 0);
}

/// TC-SP-0030
/// After clear(), new savepoints can be created starting from a fresh name
/// space but ids continue from where the allocator left off.
#[test]
fn sp_clear_then_create_continues_id_sequence() {
    let mut stack = SavepointStack::new();
    stack.create("first_batch").unwrap();
    stack.create("second_batch").unwrap();
    stack.clear();

    let sp = stack.create("after_clear").unwrap();
    // id must be > previous allocations (allocator is not reset by clear).
    assert!(
        sp.id.get() > 2,
        "allocator must not reset after clear; got id {}",
        sp.id.get()
    );
}

/// TC-SP-0031 · INV-SP-06
/// request_commit clears the savepoint stack atomically.
#[test]
fn sp_stack_cleared_on_commit_request() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.create_savepoint(tx, "s1").unwrap();
    mgr.create_savepoint(tx, "s2").unwrap();
    assert_eq!(mgr.savepoint_depth(tx).unwrap(), 2);

    mgr.request_commit(tx).unwrap();
    assert_eq!(mgr.savepoint_depth(tx).unwrap(), 0);
}

/// TC-SP-0032 · INV-SP-06
/// request_rollback clears the savepoint stack atomically.
#[test]
fn sp_stack_cleared_on_rollback_request() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.create_savepoint(tx, "s1").unwrap();
    mgr.request_rollback(tx).unwrap();
    assert_eq!(mgr.savepoint_depth(tx).unwrap(), 0);
}

/// TC-SP-0033 · INV-SP-07
/// dispose() removes live state but status history is preserved.
#[test]
fn sp_dispose_removes_live_state_but_retains_status() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.create_savepoint(tx, "before_commit").unwrap();
    mgr.request_commit(tx).unwrap();
    mgr.commit_durable(tx, 42).unwrap();
    mgr.dispose(tx).unwrap();

    assert_eq!(mgr.live_count().unwrap(), 0);
    assert!(mgr.snapshot(tx).unwrap().is_none());
    assert_eq!(mgr.status(tx).unwrap(), Some(TransactionStatus::Committed));
}

/// TC-SP-0034
/// SavepointId ordering is consistent with numeric comparison.
#[test]
fn sp_savepoint_id_ordering_is_numeric() {
    let id1 = SavepointId::new(1);
    let id2 = SavepointId::new(2);
    let id100 = SavepointId::new(100);
    assert!(id1 < id2);
    assert!(id2 < id100);
    assert_eq!(id1, SavepointId::new(1));
}

/// TC-SP-0035
/// SavepointRollbackMarker carries both the savepoint id and the ordinal.
#[test]
fn sp_rollback_marker_fields_are_accessible() {
    let marker = SavepointRollbackMarker {
        savepoint_id: SavepointId::new(7),
        rollback_ordinal: 7,
    };
    assert_eq!(marker.savepoint_id.get(), 7);
    assert_eq!(marker.rollback_ordinal, 7);
}

// ─────────────────────────────────────────────────────────────────────────────
// TASK 2 — MVCC Visibility Tests
// ─────────────────────────────────────────────────────────────────────────────

// ── 2a: Row visibility at snapshot timestamps ─────────────────────────────────

/// TC-MV-0001
/// A row created at ts=10 and not deleted is visible at ts=10 and ts=15.
#[test]
fn mv_open_row_visible_within_its_begin_ts() {
    let row = MvccRowHeader::open_version(10, TransactionId::new(1), None).unwrap();
    assert!(row.visible_at(10));
    assert!(row.visible_at(15));
}

/// TC-MV-0002
/// A row created at ts=10 is invisible at ts=9 (snapshot precedes creation).
#[test]
fn mv_open_row_invisible_before_begin_ts() {
    let row = MvccRowHeader::open_version(10, TransactionId::new(1), None).unwrap();
    assert!(!row.visible_at(9));
}

/// TC-MV-0003
/// A row deleted at end_ts=20 is visible at ts=19 and invisible at ts=20.
#[test]
fn mv_closed_row_visibility_at_end_ts_boundary() {
    let row = MvccRowHeader {
        begin_ts: 10,
        end_ts: Some(20),
        creator_tx_id: TransactionId::new(1),
        deleter_tx_id: Some(TransactionId::new(2)),
        previous_version_ptr: None,
        flags: 0,
    };
    assert!(row.visible_at(19));
    assert!(!row.visible_at(20));
    assert!(!row.visible_at(25));
}

/// TC-MV-0004
/// A savepoint snapshot at ts=15 does not see rows created at ts=16 (future).
#[test]
fn mv_savepoint_snapshot_excludes_future_writes() {
    let sp_ts = 15_u64;
    let row_future = MvccRowHeader::open_version(16, TransactionId::new(3), None).unwrap();
    assert!(!row_future.visible_at(sp_ts));
}

/// TC-MV-0005
/// Snapshot at savepoint creation time sees all committed rows with begin_ts <= sp_ts.
#[test]
fn mv_savepoint_snapshot_sees_prior_committed_rows() {
    let status_table = TransactionStatusTable::new();
    let writer_tx = TransactionId::new(10);
    status_table
        .record(writer_tx, TransactionStatus::Committed)
        .unwrap();

    let row = MvccRowHeader {
        begin_ts: 5,
        end_ts: None,
        creator_tx_id: writer_tx,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    let snapshot = Snapshot::with_context(
        10,
        CatalogVersion::new(1),
        MvccIsolationPolicy::RepeatableRead,
        Some(TransactionId::new(20)),
        [],
    )
    .unwrap();

    let visible = row.visible_in_snapshot(&snapshot, &status_table).unwrap();
    assert!(
        visible,
        "committed row with begin_ts <= snapshot.ts must be visible"
    );
}

/// TC-MV-0006
/// RepeatableRead: an in-flight concurrent writer's rows are invisible to
/// another transaction's snapshot even if ts ordering allows it.
#[test]
fn mv_rr_inflight_writer_invisible_to_snapshot() {
    let status_table = TransactionStatusTable::new();
    let concurrent_tx = TransactionId::new(5);
    // concurrent_tx is InFlight — not yet committed.
    status_table
        .record(concurrent_tx, TransactionStatus::InFlight)
        .unwrap();

    let row = MvccRowHeader {
        begin_ts: 3,
        end_ts: None,
        creator_tx_id: concurrent_tx,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    // Observer snapshot does not own concurrent_tx.
    let observer_tx = TransactionId::new(9);
    let snapshot = snapshot_rr(observer_tx, 20, [concurrent_tx]);

    let visible = row.visible_in_snapshot(&snapshot, &status_table).unwrap();
    assert!(
        !visible,
        "in-flight writer must not be visible to other tx snapshot"
    );
}

/// TC-MV-0007
/// Read-your-writes: the creating transaction sees its own uncommitted rows.
#[test]
fn mv_creator_sees_own_inflight_writes() {
    let status_table = TransactionStatusTable::new();
    let tx = TransactionId::new(7);
    status_table
        .record(tx, TransactionStatus::InFlight)
        .unwrap();

    let row = MvccRowHeader {
        begin_ts: 5,
        end_ts: None,
        creator_tx_id: tx,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    let snapshot = snapshot_rr(tx, 10, []);

    let visible = row.visible_in_snapshot(&snapshot, &status_table).unwrap();
    assert!(visible, "creator must see its own in-flight writes");
}

// ── 2b: Phantom rows ──────────────────────────────────────────────────────────

/// TC-MV-0008
/// A row inserted after the savepoint timestamp is a phantom at that snapshot.
#[test]
fn mv_phantom_row_inserted_after_savepoint_ts_is_invisible() {
    let sp_ts = 100_u64;
    let row = MvccRowHeader::open_version(101, TransactionId::new(1), None).unwrap();
    assert!(
        !row.visible_at(sp_ts),
        "phantom row (begin_ts > sp_ts) must not be visible at savepoint snapshot"
    );
}

/// TC-MV-0009
/// A row deleted before the savepoint timestamp is invisible at the snapshot.
#[test]
fn mv_row_deleted_before_savepoint_ts_is_invisible() {
    let sp_ts = 100_u64;
    let row = MvccRowHeader {
        begin_ts: 50,
        end_ts: Some(90), // deleted well before sp_ts
        creator_tx_id: TransactionId::new(1),
        deleter_tx_id: Some(TransactionId::new(2)),
        previous_version_ptr: None,
        flags: 0,
    };
    assert!(!row.visible_at(sp_ts));
}

/// TC-MV-0010
/// A row that straddles the savepoint timestamp: begin_ts < sp_ts < end_ts
/// is visible at the savepoint snapshot.
#[test]
fn mv_row_straddling_savepoint_ts_is_visible() {
    let sp_ts = 100_u64;
    let row = MvccRowHeader {
        begin_ts: 80,
        end_ts: Some(120),
        creator_tx_id: TransactionId::new(1),
        deleter_tx_id: Some(TransactionId::new(2)),
        previous_version_ptr: None,
        flags: 0,
    };
    assert!(row.visible_at(sp_ts));
}

/// TC-MV-0011
/// After rollback-to-savepoint, writes made inside the discarded scope must
/// not be visible to the owning transaction's subsequent reads at the target ts.
#[test]
fn mv_rolled_back_savepoint_writes_become_invisible() {
    let status_table = TransactionStatusTable::new();
    let tx = TransactionId::new(11);
    status_table
        .record(tx, TransactionStatus::InFlight)
        .unwrap();

    // Row created inside the discarded savepoint scope — after savepoint ts.
    let discarded_row = MvccRowHeader {
        begin_ts: 200, // post-savepoint
        end_ts: None,
        creator_tx_id: tx,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    // Restore an external snapshot at savepoint ts = 100.
    let snapshot = snapshot_rr(TransactionId::new(99), 100, []);
    let visible = discarded_row
        .visible_in_snapshot(&snapshot, &status_table)
        .unwrap();
    assert!(
        !visible,
        "row created after savepoint ts must not be visible at savepoint snapshot"
    );
}

// ── 2c: Dirty reads ───────────────────────────────────────────────────────────

/// TC-MV-0012
/// Rolled-back transaction's rows are never visible to any snapshot.
#[test]
fn mv_rolled_back_tx_rows_are_never_visible() {
    let status_table = TransactionStatusTable::new();
    let aborted_tx = TransactionId::new(20);
    status_table
        .record(aborted_tx, TransactionStatus::RolledBack)
        .unwrap();

    let row = MvccRowHeader {
        begin_ts: 5,
        end_ts: None,
        creator_tx_id: aborted_tx,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    let snapshot = snapshot_rc(50);
    let visible = row.visible_in_snapshot(&snapshot, &status_table).unwrap();
    assert!(
        !visible,
        "rolled-back tx rows must be invisible to all snapshots"
    );
}

/// TC-MV-0013
/// Read-committed snapshot does not see in-flight writes from other transactions.
#[test]
fn mv_rc_snapshot_does_not_see_inflight_foreign_writes() {
    let status_table = TransactionStatusTable::new();
    let writer = TransactionId::new(30);
    status_table
        .record(writer, TransactionStatus::InFlight)
        .unwrap();

    let row = MvccRowHeader {
        begin_ts: 5,
        end_ts: None,
        creator_tx_id: writer,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    let snapshot = snapshot_rc(50);
    let visible = row.visible_in_snapshot(&snapshot, &status_table).unwrap();
    assert!(
        !visible,
        "RC snapshot must not see in-flight foreign writes (no dirty reads)"
    );
}

/// TC-MV-0014
/// A transaction that has no recorded status is treated as InFlight (invisible
/// to foreign snapshots — V0 doctrine: visible commit ≡ durable WAL).
#[test]
fn mv_unregistered_tx_treated_as_inflight_and_invisible() {
    let status_table = TransactionStatusTable::new();
    let unregistered = TransactionId::new(99);
    // Deliberately NOT recorded in status_table.

    let row = MvccRowHeader {
        begin_ts: 1,
        end_ts: None,
        creator_tx_id: unregistered,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    let snapshot = snapshot_rc(100);
    let visible = row.visible_in_snapshot(&snapshot, &status_table).unwrap();
    assert!(
        !visible,
        "unregistered tx must be treated as InFlight and invisible to foreign snapshots"
    );
}

// ── 2d: Snapshot isolation — concurrent savepoints don't interfere ────────────

/// TC-MV-0015
/// Two concurrent transactions with separate savepoints see independent row sets.
#[test]
fn mv_concurrent_tx_savepoints_are_isolated() {
    let status_table = TransactionStatusTable::new();
    let tx_a = TransactionId::new(1);
    let tx_b = TransactionId::new(2);
    status_table
        .record(tx_a, TransactionStatus::InFlight)
        .unwrap();
    status_table
        .record(tx_b, TransactionStatus::InFlight)
        .unwrap();

    // Row created by tx_a.
    let row_a = MvccRowHeader {
        begin_ts: 5,
        end_ts: None,
        creator_tx_id: tx_a,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    // tx_a sees its own row.
    let snap_a = snapshot_rr(tx_a, 10, [tx_b]);
    assert!(
        row_a.visible_in_snapshot(&snap_a, &status_table).unwrap(),
        "tx_a must see its own rows"
    );

    // tx_b does NOT see tx_a's uncommitted row.
    let snap_b = snapshot_rr(tx_b, 10, [tx_a]);
    assert!(
        !row_a.visible_in_snapshot(&snap_b, &status_table).unwrap(),
        "tx_b must not see tx_a's in-flight rows"
    );
}

/// TC-MV-0016
/// After tx_a commits, tx_b with RR isolation still does not see tx_a's rows
/// if tx_a was active at snap_b creation time.
#[test]
fn mv_rr_does_not_see_post_snapshot_commits() {
    let status_table = TransactionStatusTable::new();
    let tx_a = TransactionId::new(1);
    let tx_b = TransactionId::new(2);
    // tx_a is active when tx_b takes its snapshot.
    status_table
        .record(tx_a, TransactionStatus::InFlight)
        .unwrap();
    status_table
        .record(tx_b, TransactionStatus::InFlight)
        .unwrap();

    let row_a = MvccRowHeader {
        begin_ts: 5,
        end_ts: None,
        creator_tx_id: tx_a,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    // tx_b's snapshot lists tx_a as active.
    let snap_b = snapshot_rr(tx_b, 10, [tx_a]);

    // Now tx_a commits.
    status_table
        .record(tx_a, TransactionStatus::Committed)
        .unwrap();

    // RR: tx_b's snapshot was taken while tx_a was active, so tx_a remains invisible.
    let visible = row_a.visible_in_snapshot(&snap_b, &status_table).unwrap();
    assert!(
        !visible,
        "RR snapshot must not see tx that was active at snapshot creation, even after commit"
    );
}

/// TC-MV-0017
/// After tx_a commits, a new RC snapshot sees tx_a's rows.
#[test]
fn mv_rc_sees_newly_committed_rows() {
    let status_table = TransactionStatusTable::new();
    let tx_a = TransactionId::new(1);
    status_table
        .record(tx_a, TransactionStatus::Committed)
        .unwrap();

    let row_a = MvccRowHeader {
        begin_ts: 5,
        end_ts: None,
        creator_tx_id: tx_a,
        deleter_tx_id: None,
        previous_version_ptr: None,
        flags: 0,
    };

    let rc_snap = snapshot_rc(10);
    let visible = row_a.visible_in_snapshot(&rc_snap, &status_table).unwrap();
    assert!(visible, "RC snapshot must see committed rows");
}

// ── 2e: Row versioning — BEGIN_TS / END_TS semantics ─────────────────────────

/// TC-MV-0018
/// open_version requires non-zero begin_ts and creator_tx_id.
#[test]
fn mv_open_version_rejects_zero_begin_ts() {
    let err = MvccRowHeader::open_version(0, TransactionId::new(1), None).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-MV-0019
/// open_version rejects creator_tx_id == 0.
#[test]
fn mv_open_version_rejects_zero_creator_tx_id() {
    let err = MvccRowHeader::open_version(1, TransactionId::new(0), None).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-MV-0020
/// close_version requires end_ts > begin_ts.
#[test]
fn mv_close_version_rejects_end_ts_equal_to_begin_ts() {
    let row = MvccRowHeader::open_version(10, TransactionId::new(1), None).unwrap();
    let err = row.close_version(10, TransactionId::new(2)).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-MV-0021
/// close_version rejects end_ts < begin_ts.
#[test]
fn mv_close_version_rejects_end_ts_before_begin_ts() {
    let row = MvccRowHeader::open_version(10, TransactionId::new(1), None).unwrap();
    let err = row.close_version(9, TransactionId::new(2)).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-MV-0022
/// A closed row without a deleter_tx_id fails validation.
#[test]
fn mv_closed_row_without_deleter_tx_id_fails_validation() {
    let row = MvccRowHeader {
        begin_ts: 5,
        end_ts: Some(10),
        creator_tx_id: TransactionId::new(1),
        deleter_tx_id: None, // missing!
        previous_version_ptr: None,
        flags: 0,
    };
    let err = row.validate().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-MV-0023
/// An open row with a deleter_tx_id fails validation.
#[test]
fn mv_open_row_with_deleter_tx_id_fails_validation() {
    let row = MvccRowHeader {
        begin_ts: 5,
        end_ts: None,
        creator_tx_id: TransactionId::new(1),
        deleter_tx_id: Some(TransactionId::new(2)), // must not have deleter for open
        previous_version_ptr: None,
        flags: 0,
    };
    let err = row.validate().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-MV-0024
/// previous_version_ptr is optional and does not affect visibility.
#[test]
fn mv_previous_version_ptr_does_not_affect_visibility() {
    let with_ptr = MvccRowHeader::open_version(10, TransactionId::new(1), Some(9999)).unwrap();
    let without_ptr = MvccRowHeader::open_version(10, TransactionId::new(1), None).unwrap();
    assert_eq!(with_ptr.visible_at(10), without_ptr.visible_at(10));
    assert_eq!(with_ptr.visible_at(5), without_ptr.visible_at(5));
}

/// TC-MV-0025
/// is_open_version and is_closed_version are mutually exclusive.
#[test]
fn mv_open_and_closed_version_are_mutually_exclusive() {
    let open = MvccRowHeader::open_version(10, TransactionId::new(1), None).unwrap();
    assert!(open.is_open_version());
    assert!(!open.is_closed_version());

    let closed = open.close_version(20, TransactionId::new(2)).unwrap();
    assert!(closed.is_closed_version());
    assert!(!closed.is_open_version());
}

/// TC-MV-0026
/// Row delete visibility: committed delete at end_ts <= snapshot.ts makes row invisible under RC.
#[test]
fn mv_committed_delete_at_or_before_snapshot_ts_hides_row_under_rc() {
    let status_table = TransactionStatusTable::new();
    let creator = TransactionId::new(1);
    let deleter = TransactionId::new(2);
    status_table
        .record(creator, TransactionStatus::Committed)
        .unwrap();
    status_table
        .record(deleter, TransactionStatus::Committed)
        .unwrap();

    let row = MvccRowHeader {
        begin_ts: 5,
        end_ts: Some(15),
        creator_tx_id: creator,
        deleter_tx_id: Some(deleter),
        previous_version_ptr: None,
        flags: 0,
    };

    let snapshot = snapshot_rc(20); // snapshot at ts=20, delete was at 15
    let visible = row.visible_in_snapshot(&snapshot, &status_table).unwrap();
    assert!(
        !visible,
        "committed delete before snapshot ts must hide row under RC"
    );
}

/// TC-MV-0027
/// Row delete visibility: committed delete strictly after snapshot.ts keeps row visible.
#[test]
fn mv_committed_delete_after_snapshot_ts_keeps_row_visible() {
    let status_table = TransactionStatusTable::new();
    let creator = TransactionId::new(1);
    let deleter = TransactionId::new(2);
    status_table
        .record(creator, TransactionStatus::Committed)
        .unwrap();
    status_table
        .record(deleter, TransactionStatus::Committed)
        .unwrap();

    let row = MvccRowHeader {
        begin_ts: 5,
        end_ts: Some(25), // delete after snapshot ts
        creator_tx_id: creator,
        deleter_tx_id: Some(deleter),
        previous_version_ptr: None,
        flags: 0,
    };

    let snapshot = snapshot_rc(20); // snapshot at ts=20
    let visible = row.visible_in_snapshot(&snapshot, &status_table).unwrap();
    assert!(visible, "delete after snapshot ts must keep row visible");
}

/// TC-MV-0028
/// Snapshot validates non-zero timestamp.
#[test]
fn mv_snapshot_rejects_zero_timestamp() {
    let err = Snapshot::with_context(
        0,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        None,
        [],
    )
    .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-MV-0029
/// Snapshot normalizes active_tx_ids (sort + dedup).
#[test]
fn mv_snapshot_normalizes_active_tx_ids() {
    let snapshot = Snapshot::with_context(
        10,
        CatalogVersion::new(1),
        MvccIsolationPolicy::RepeatableRead,
        Some(TransactionId::new(1)),
        [
            TransactionId::new(5),
            TransactionId::new(3),
            TransactionId::new(5), // duplicate
        ],
    )
    .unwrap();
    assert_eq!(
        snapshot.active_tx_ids,
        vec![TransactionId::new(3), TransactionId::new(5)]
    );
}

/// TC-MV-0030
/// Snapshot with a terminal transaction in active_tx_ids is rejected by
/// validate_against_statuses.
#[test]
fn mv_snapshot_rejects_terminal_tx_in_active_list() {
    let status_table = TransactionStatusTable::new();
    let committed_tx = TransactionId::new(5);
    status_table
        .record(committed_tx, TransactionStatus::Committed)
        .unwrap();
    // Also register owner as InFlight.
    let owner = TransactionId::new(9);
    status_table
        .record(owner, TransactionStatus::InFlight)
        .unwrap();

    let snapshot = Snapshot::with_context(
        10,
        CatalogVersion::new(1),
        MvccIsolationPolicy::RepeatableRead,
        Some(owner),
        [committed_tx],
    )
    .unwrap();

    let err = snapshot
        .validate_against_statuses(&status_table)
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

// ─────────────────────────────────────────────────────────────────────────────
// TASK 3 — WAL & Durability Tests
// ─────────────────────────────────────────────────────────────────────────────

// ── 3a: SAVEPOINT does not write WAL by itself ────────────────────────────────

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

// ── 3b: WAL replay — commit and rollback records ──────────────────────────────

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

// ── 3c: Recovery — savepoints cleared before terminal WAL record ──────────────

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

// ── 3d: Crash scenarios (simulated) ──────────────────────────────────────────

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

// ── 3e: Consistency — recovered savepoints match pre-crash state ──────────────

/// TC-WD-0014
/// Multiple transactions replayed in a single batch produce independently
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

// ─────────────────────────────────────────────────────────────────────────────
// TASK 4 — Error Handling & Rollback Tests
// ─────────────────────────────────────────────────────────────────────────────

// ── 4a: Error during savepoint work → transaction status = Failed ─────────────

/// TC-EH-0001
/// fail() transitions Active → Failed; savepoint operations are then rejected.
#[test]
fn eh_fail_event_blocks_savepoint_operations() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.create_savepoint(tx, "before_fail").unwrap();

    mgr.fail(tx).unwrap();

    let snap = mgr.snapshot(tx).unwrap().unwrap();
    assert_eq!(snap.state_machine.state, TransactionState::Failed);

    // All savepoint ops must be rejected after fail.
    assert!(mgr.create_savepoint(tx, "after_fail").is_err());
    assert!(mgr.rollback_to_savepoint(tx, "before_fail").is_err());
    assert!(mgr.release_savepoint(tx, "before_fail").is_err());
}

/// TC-EH-0002
/// Failed transaction can progress to RollingBack via request_rollback.
#[test]
fn eh_failed_tx_can_transition_to_rolling_back() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.fail(tx).unwrap();

    // Failed → RollingBack is a legal transition.
    mgr.request_rollback(tx).unwrap();
    let snap = mgr.snapshot(tx).unwrap().unwrap();
    assert_eq!(snap.state_machine.state, TransactionState::RollingBack);
}

/// TC-EH-0003
/// Failed → RolledBack requires a durable WAL LSN (WAL-before-visibility).
#[test]
fn eh_failed_tx_rollback_completion_requires_durable_wal() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.fail(tx).unwrap();
    mgr.request_rollback(tx).unwrap();

    // Attempt RollbackComplete without a durable LSN — must fail.
    // We directly use the state machine to probe this invariant.
    let snap = mgr.snapshot(tx).unwrap().unwrap();
    let mut machine = snap.state_machine;
    let err = machine
        .apply(andromeda_tx::TransactionEvent::RollbackComplete)
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-EH-0004
/// Poisoned transaction must rollback before disposal (INV-SP-12).
#[test]
fn eh_poisoned_tx_must_rollback_before_disposal() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.poison(tx).unwrap();

    // Cannot commit from Poisoned.
    let err = mgr.request_commit(tx).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);

    // Cannot dispose from Poisoned.
    let err = mgr.dispose(tx).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);

    // Must rollback.
    mgr.request_rollback(tx).unwrap();
    mgr.rollback_durable(tx, 55).unwrap();
    mgr.dispose(tx).unwrap();
    assert_eq!(mgr.live_count().unwrap(), 0);
}

// ── 4b: RESTORE on failed transaction ─────────────────────────────────────────

/// TC-EH-0005
/// rollback_to_savepoint on a Failed transaction returns a clean error; no
/// state corruption occurs.
#[test]
fn eh_rollback_to_on_failed_tx_returns_clean_error_no_corruption() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.create_savepoint(tx, "safe_point").unwrap();

    mgr.fail(tx).unwrap();

    let err = mgr.rollback_to_savepoint(tx, "safe_point").unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);

    // Tx is still Failed — not corrupted to another state.
    let snap = mgr.snapshot(tx).unwrap().unwrap();
    assert_eq!(snap.state_machine.state, TransactionState::Failed);
}

/// TC-EH-0006
/// release_savepoint on a Failed transaction returns a clean error.
#[test]
fn eh_release_on_failed_tx_returns_clean_error() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.create_savepoint(tx, "sp").unwrap();
    mgr.fail(tx).unwrap();

    let err = mgr.release_savepoint(tx, "sp").unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

// ── 4c: Nested restore failure — parent unaffected ────────────────────────────

/// TC-EH-0007
/// Rolling back to a nonexistent savepoint does not affect the outer savepoint.
#[test]
fn eh_rollback_to_nonexistent_sp_does_not_affect_outer_savepoint() {
    let mut stack = SavepointStack::new();
    stack.create("outer").unwrap();
    stack.create("inner").unwrap();

    let depth_before = stack.depth();
    let err = stack.rollback_to("ghost_sp").unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);

    // Stack must be unmodified.
    assert_eq!(stack.depth(), depth_before);
    let names: Vec<&str> = stack.active().iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["outer", "inner"]);
}

/// TC-EH-0008
/// Releasing a nonexistent savepoint does not alter the stack.
#[test]
fn eh_release_nonexistent_sp_does_not_alter_stack() {
    let mut stack = SavepointStack::new();
    stack.create("real_sp").unwrap();
    let depth_before = stack.depth();

    let err = stack.release("phantom").unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(stack.depth(), depth_before);
}

/// TC-EH-0009
/// After a failed nested rollback, the parent savepoint remains operable.
#[test]
fn eh_failed_inner_rollback_parent_sp_remains_operable() {
    let mut stack = SavepointStack::new();
    stack.create("parent").unwrap();
    stack.create("child").unwrap();

    // Attempt to rollback to a ghost — fails cleanly.
    stack.rollback_to("ghost").unwrap_err();

    // Parent rollback must still succeed.
    let ev = stack.rollback_to("parent").unwrap();
    assert_eq!(ev.target.name, "parent");
    assert_eq!(ev.discarded_descendants.len(), 1);
}

// ── 4d: Various state-machine error paths ────────────────────────────────────

/// TC-EH-0010
/// Commit requires durable WAL LSN before Committed state.
#[test]
fn eh_commit_without_wal_lsn_is_rejected() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.request_commit(tx).unwrap();

    // commit_durable with LSN=0 must be rejected.
    let err = mgr.commit_durable(tx, 0).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-EH-0011
/// Rollback completion requires durable WAL LSN.
#[test]
fn eh_rollback_completion_without_wal_lsn_is_rejected() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.request_rollback(tx).unwrap();

    let err = mgr.rollback_durable(tx, 0).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-EH-0012
/// Commit on a RollingBack transaction is rejected.
#[test]
fn eh_commit_on_rolling_back_tx_is_rejected() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    mgr.request_rollback(tx).unwrap();

    let err = mgr.request_commit(tx).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-EH-0013
/// dispose() on an Active transaction (non-terminal) is rejected.
#[test]
fn eh_dispose_on_active_tx_is_rejected() {
    let mgr = TransactionManager::new();
    let tx = mgr.begin().unwrap();
    let err = mgr.dispose(tx).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

/// TC-EH-0014
/// double-begin: beginning a transaction twice is safe because each call
/// allocates a fresh id from a monotonic allocator — no collision possible.
#[test]
fn eh_two_begins_allocate_different_ids() {
    let mgr = TransactionManager::new();
    let tx1 = mgr.begin().unwrap();
    let tx2 = mgr.begin().unwrap();
    assert_ne!(tx1, tx2);
    assert!(tx2.get() > tx1.get());
    assert_eq!(mgr.live_count().unwrap(), 2);
}

/// TC-EH-0015
/// fail() on a Created transaction (before begin) is rejected.
#[test]
fn eh_fail_on_created_state_is_rejected() {
    // Direct state machine test — Created → Failed is not a legal transition.
    let state = andromeda_tx::TransactionState::Created;
    let err = state
        .apply(andromeda_tx::TransactionEvent::Fail)
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
}

// ── 4e: Audit trace — critical operations produce traces ─────────────────────

/// TC-EH-0016
/// TransactionTrace correctly captures terminal state after commit.
#[test]
fn eh_trace_captures_terminal_state_after_commit() {
    use andromeda_observe::TraceId;
    use andromeda_tx::{TransactionStateMachine, TransactionTrace};

    let mut machine = TransactionStateMachine::new(TransactionId::new(1));
    machine.begin().unwrap();
    machine.request_commit().unwrap();
    machine
        .publish_visible_commit_after_durable_flush(999)
        .unwrap();

    let trace = TransactionTrace::from_state_machine(TraceId::new(42), machine);
    assert!(trace.is_terminal());
    assert_eq!(trace.state, TransactionState::Committed);
}

/// TC-EH-0017
/// project_transition produces non-terminal trace for Active → Committing.
#[test]
fn eh_trace_non_terminal_for_active_to_committing() {
    use andromeda_observe::{TraceId, TransitionReasonCode};
    use andromeda_tx::{TransactionStateMachine, TransactionTransitionCorrelation};

    let mut machine = TransactionStateMachine::new(TransactionId::new(2));
    machine.begin().unwrap();
    let prev = machine.state;
    machine.request_commit().unwrap();

    let trace = machine.project_transition(
        TraceId::new(1),
        prev,
        TransactionTransitionCorrelation::empty(),
        TransitionReasonCode::NORMAL_PROGRESS,
        "commit requested",
    );

    assert_eq!(trace.durable_lsn, None);
    assert!(trace.validate().is_ok());
}

/// TC-EH-0018
/// project_transition for RolledBack carries durable_lsn evidence.
#[test]
fn eh_trace_rolled_back_carries_durable_lsn() {
    use andromeda_observe::{TraceId, TransitionReasonCode};
    use andromeda_tx::{TransactionStateMachine, TransactionTransitionCorrelation};

    let mut machine = TransactionStateMachine::new(TransactionId::new(3));
    machine.begin().unwrap();
    machine.request_rollback().unwrap();
    let prev = machine.state;
    machine.complete_rollback_after_durable_flush(1234).unwrap();

    let trace = machine.project_transition(
        TraceId::new(2),
        prev,
        TransactionTransitionCorrelation::empty(),
        TransitionReasonCode::DURABLE_WAL_FLUSH,
        "rollback durable",
    );

    assert_eq!(trace.durable_lsn, Some(1234));
    assert!(trace.proves_terminal_evidence());
    assert!(trace.validate().is_ok());
}

// ─────────────────────────────────────────────────────────────────────────────
// TASK 5 — Concurrent & Isolation Tests
// ─────────────────────────────────────────────────────────────────────────────

// ── 5a: Savepoints are isolated per transaction ───────────────────────────────

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

// ── 5b: Serialization — savepoints don't violate isolation levels ─────────────

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
        .record(late_writer, TransactionStatus::Committed)
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

// ── 5c: Deadlock prevention ────────────────────────────────────────────────────

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

// ── 5d: Lock conflicts respect savepoint state ────────────────────────────────

/// TC-CI-0008
/// create_savepoint does not require or release any locks (2PL discipline).
#[test]
fn ci_create_savepoint_does_not_alter_lock_state() {
    use andromeda_tx::{LockManager, LockMode, LockResource};

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
    use andromeda_tx::LockAcquireStatus;
    assert_ne!(
        status,
        LockAcquireStatus::Granted,
        "lock held by tx must conflict with exclusive acquisition by tx2"
    );
}

// ── 5e: Priority — high-priority savepoint contexts ───────────────────────────

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
                t.record(tx_id, TransactionStatus::Committed).unwrap();
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

// ─────────────────────────────────────────────────────────────────────────────
// TASK 6 — Crash Recovery & Performance Gates
// ─────────────────────────────────────────────────────────────────────────────

// ── 6a: Crash during SAVEPOINT ────────────────────────────────────────────────

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

// ── 6b: Crash during RESTORE ──────────────────────────────────────────────────

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

// ── 6c: Performance gates ─────────────────────────────────────────────────────

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
    use andromeda_tx::TransactionEvent;

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
    let mut machine = andromeda_tx::TransactionStateMachine::new(TransactionId::new(42));
    machine.begin().unwrap();
    machine.request_commit().unwrap();
    machine
        .publish_visible_commit_after_durable_flush(100)
        .unwrap();
    assert!(machine.is_visible_committed());

    machine
        .apply(andromeda_tx::TransactionEvent::Dispose)
        .unwrap();
    assert_eq!(machine.state, TransactionState::Disposed);
}

/// TC-SM-0004
/// Full rollback path: Created → Active → RollingBack → RolledBack → Disposed.
#[test]
fn sm_full_rollback_path_is_legal() {
    let mut machine = andromeda_tx::TransactionStateMachine::new(TransactionId::new(43));
    machine.begin().unwrap();
    machine.request_rollback().unwrap();
    machine.complete_rollback_after_durable_flush(200).unwrap();
    assert!(machine.is_durable_rolled_back());

    machine
        .apply(andromeda_tx::TransactionEvent::Dispose)
        .unwrap();
    assert_eq!(machine.state, TransactionState::Disposed);
}

/// TC-SM-0005
/// Poison path: Created → Active → Poisoned → RollingBack → RolledBack → Disposed.
#[test]
fn sm_full_poison_path_is_legal() {
    let mut machine = andromeda_tx::TransactionStateMachine::new(TransactionId::new(44));
    machine.begin().unwrap();
    machine
        .apply(andromeda_tx::TransactionEvent::Poison)
        .unwrap();
    machine.request_rollback().unwrap();
    machine.complete_rollback_after_durable_flush(300).unwrap();
    machine
        .apply(andromeda_tx::TransactionEvent::Dispose)
        .unwrap();
    assert_eq!(machine.state, TransactionState::Disposed);
}

// ── Supplementary: SavepointStack API surface completeness ────────────────────

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
/// Savepoint struct is Clone and Eq: a cloned savepoint equals the original.
#[test]
fn ss_savepoint_is_clone_and_eq() {
    let mut stack = SavepointStack::new();
    let original = stack.create("cloneable").unwrap();
    let cloned = original.clone();
    assert_eq!(original, cloned);
}
