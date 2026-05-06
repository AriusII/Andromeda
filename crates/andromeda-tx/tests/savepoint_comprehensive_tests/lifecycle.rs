use super::*;

// TASK 1 — Savepoint Lifecycle Unit Tests

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
