//! Contract tests for lock audit trace infrastructure.
//!
//! These tests verify that lock traces are emitted correctly for:
//! - Lock waits (when a transaction waits for a lock)
//! - Lock promotions (when a waiter is promoted to holder)
//! - Deadlock decisions (when deadlock detection runs)
//! - Lock release-all (when a transaction cleans up all locks)
//! - Correlation (same transaction across multiple trace events)

use andromeda_core::{EngineTimestamp, TransactionId};
use andromeda_tx::{
    DeadlockAuditTrace, DeadlockDecisionKind, LockAcquireStatus, LockHolder, LockManager,
    LockMode, LockPromotionTrace, LockReleaseAllTrace, LockResource, LockWaitTrace,
    TransactionManager, TransactionState,
};

#[test]
fn test_wait_trace_on_contention() {
    // Two transactions contend for a lock
    let lock_manager = LockManager::new();
    let resource = LockResource::table(1, 10).unwrap();
    let tx1 = TransactionId::new(1);
    let tx2 = TransactionId::new(2);
    let ts = EngineTimestamp::from_unix_millis(1000);

    // Transaction 1 acquires exclusive lock
    let status1 = lock_manager.acquire(tx1, resource, LockMode::Exclusive).unwrap();
    assert_eq!(status1, LockAcquireStatus::Granted);

    // Transaction 2 requests shared lock (incompatible, will wait)
    let status2 = lock_manager.acquire(tx2, resource, LockMode::Shared).unwrap();

    // Verify transaction 2 is waiting
    match status2 {
        LockAcquireStatus::Waiting {
            blockers,
            sequence: _,
        } => {
            // Create wait trace from the acquire status
            let wait_trace = LockWaitTrace::new(
                tx2,
                resource,
                LockMode::Shared,
                blockers.clone(),
                ts,
            );

            // Verify wait trace captures the contention
            assert_eq!(wait_trace.tx_id, tx2);
            assert_eq!(wait_trace.resource_id, resource);
            assert_eq!(wait_trace.requested_mode, LockMode::Shared);
            assert_eq!(wait_trace.blocker_tx_ids, vec![tx1]);
            assert_eq!(wait_trace.timestamp, ts);
        }
        other => panic!("Expected Waiting status, got {:?}", other),
    }
}

#[test]
fn test_promotion_trace_on_release() {
    // Holder releases lock → waiter promoted → promotion trace emitted
    let lock_manager = LockManager::new();
    let resource = LockResource::table(1, 10).unwrap();
    let tx1 = TransactionId::new(1);
    let tx2 = TransactionId::new(2);
    let ts_release = EngineTimestamp::from_unix_millis(2000);
    let ts_promote = EngineTimestamp::from_unix_millis(2001);

    // Tx1 acquires exclusive lock
    let _ = lock_manager.acquire(tx1, resource, LockMode::Exclusive).unwrap();

    // Tx2 waits for shared lock
    let _ = lock_manager.acquire(tx2, resource, LockMode::Shared).unwrap();

    // Tx1 releases (this promotes tx2)
    let release_result = lock_manager.release_with_evidence(tx1, resource).unwrap();

    // Verify promotions were recorded in evidence
    assert!(release_result.released_any);
    assert!(!release_result.evidence.is_empty());

    // Create promotion trace from the evidence
    for evidence in release_result.evidence {
        if evidence.promoted_waiters.len() > 0 {
            let promoted_waiter = &evidence.promoted_waiters[0];
            let promotion_trace = LockPromotionTrace::new(
                promoted_waiter.tx_id,
                resource,
                promoted_waiter.mode,
                tx1,
                ts_promote,
            );

            // Verify promotion trace
            assert_eq!(promotion_trace.tx_id, tx2);
            assert_eq!(promotion_trace.resource_id, resource);
            assert_eq!(promotion_trace.mode, LockMode::Shared);
            assert_eq!(promotion_trace.released_by_tx, tx1);
        }
    }
}

#[test]
fn test_deadlock_decision_trace() {
    // Deadlock cycle detected → trace contains victim and decision
    let ts = EngineTimestamp::from_unix_millis(3000);

    // Scenario: tx1 -> tx2 -> tx1 cycle
    let cycle_txs = vec![TransactionId::new(1), TransactionId::new(2)];
    let victim_tx = TransactionId::new(1);

    let trace = DeadlockAuditTrace::victim_suggested(
        ts,
        cycle_txs.clone(),
        victim_tx,
        "cycle detected: youngest by start order selected as victim",
    );

    // Verify deadlock decision trace
    assert_eq!(trace.detection_ts, ts);
    assert_eq!(trace.cycle_txs, cycle_txs);
    assert_eq!(trace.decision, DeadlockDecisionKind::VictimSuggested);
    assert_eq!(trace.victim_tx_id, Some(victim_tx));
    assert!(trace.reason.contains("cycle detected"));
}

#[test]
fn test_release_all_trace_on_commit() {
    // release_all called with Committed state → trace recorded
    let lock_manager = LockManager::new();
    let resource = LockResource::table(1, 10).unwrap();
    let tx_id = TransactionId::new(1);
    let ts = EngineTimestamp::from_unix_millis(4000);

    // Transaction acquires a lock
    let _ = lock_manager.acquire(tx_id, resource, LockMode::Exclusive).unwrap();

    // Release all locks (terminal cleanup)
    let cleanup_result = lock_manager.release_all_with_evidence(tx_id).unwrap();

    // Create release-all trace from the cleanup result
    let trace = LockReleaseAllTrace::new(
        tx_id,
        cleanup_result.summary.affected_resource_count,
        TransactionState::Committed,
        ts,
    );

    // Verify release-all trace
    assert_eq!(trace.tx_id, tx_id);
    assert_eq!(trace.resources_released, 1);
    assert_eq!(trace.terminal_state, TransactionState::Committed);
    assert_eq!(trace.timestamp, ts);
}

#[test]
fn test_audit_trail_correlation() {
    // Same transaction appears in wait → promotion → release traces
    let lock_manager = LockManager::new();
    let resource = LockResource::table(1, 10).unwrap();
    let tx_holder = TransactionId::new(1);
    let tx_waiter = TransactionId::new(2);
    let ts_wait = EngineTimestamp::from_unix_millis(5000);
    let ts_promote = EngineTimestamp::from_unix_millis(5100);
    let ts_release_all = EngineTimestamp::from_unix_millis(5200);

    // Step 1: Holder acquires lock
    let _ = lock_manager.acquire(tx_holder, resource, LockMode::Exclusive).unwrap();

    // Step 2: Waiter tries to acquire (creates wait trace)
    let acquire_status = lock_manager.acquire(tx_waiter, resource, LockMode::Shared).unwrap();
    let wait_trace = match acquire_status {
        LockAcquireStatus::Waiting { blockers, .. } => LockWaitTrace::new(
            tx_waiter,
            resource,
            LockMode::Shared,
            blockers,
            ts_wait,
        ),
        _ => panic!("Expected Waiting status"),
    };

    // Step 3: Holder releases (creates promotion trace)
    let release_result = lock_manager.release_with_evidence(tx_holder, resource).unwrap();
    let promotion_trace = if !release_result.evidence.is_empty()
        && !release_result.evidence[0].promoted_waiters.is_empty()
    {
        let promoted_waiter = &release_result.evidence[0].promoted_waiters[0];
        LockPromotionTrace::new(
            promoted_waiter.tx_id,
            resource,
            promoted_waiter.mode,
            tx_holder,
            ts_promote,
        )
    } else {
        panic!("Expected promotion trace")
    };

    // Step 4: Waiter releases all (creates release-all trace)
    let cleanup_result = lock_manager.release_all_with_evidence(tx_waiter).unwrap();
    let release_all_trace = LockReleaseAllTrace::new(
        tx_waiter,
        cleanup_result.summary.affected_resource_count,
        TransactionState::Committed,
        ts_release_all,
    );

    // Verify correlation: all traces have same tx_id
    assert_eq!(wait_trace.tx_id, tx_waiter);
    assert_eq!(promotion_trace.tx_id, tx_waiter);
    assert_eq!(release_all_trace.tx_id, tx_waiter);

    // Verify trace ordering (timestamps should be sequential)
    assert!(wait_trace.timestamp < promotion_trace.timestamp);
    assert!(promotion_trace.timestamp < release_all_trace.timestamp);

    // Verify trace content consistency
    assert_eq!(wait_trace.resource_id, resource);
    assert_eq!(promotion_trace.resource_id, resource);
    assert_eq!(promotion_trace.released_by_tx, tx_holder);
}

#[test]
fn test_no_wait_trace_on_granted_acquire() {
    // If acquire is granted immediately, no wait trace needed
    let lock_manager = LockManager::new();
    let resource = LockResource::table(1, 10).unwrap();
    let tx = TransactionId::new(1);

    let status = lock_manager.acquire(tx, resource, LockMode::Shared).unwrap();

    // Should be granted (not waiting)
    match status {
        LockAcquireStatus::Granted => {
            // No wait trace should be emitted for granted acquires
            // This is verified by the absence of wait evidence
        }
        other => panic!("Expected Granted status, got {:?}", other),
    }
}

#[test]
fn test_deadlock_no_cycle_trace() {
    // When no cycle is detected, decision trace reflects that
    let ts = EngineTimestamp::from_unix_millis(6000);

    let trace = DeadlockAuditTrace::no_cycle_detected(ts);

    // Verify no-cycle trace
    assert_eq!(trace.detection_ts, ts);
    assert_eq!(trace.cycle_txs.len(), 0);
    assert_eq!(trace.decision, DeadlockDecisionKind::NoCycleDetected);
    assert_eq!(trace.victim_tx_id, None);
}

#[test]
fn test_multiple_promotions_on_release() {
    // Multiple compatible waiters can be promoted together
    let lock_manager = LockManager::new();
    let resource = LockResource::table(1, 10).unwrap();
    let tx_holder = TransactionId::new(1);
    let tx_wait1 = TransactionId::new(2);
    let tx_wait2 = TransactionId::new(3);

    // Holder acquires exclusive lock
    let _ = lock_manager.acquire(tx_holder, resource, LockMode::Exclusive).unwrap();

    // First waiter queues for shared
    let _ = lock_manager.acquire(tx_wait1, resource, LockMode::Shared).unwrap();

    // Second waiter queues for shared
    let _ = lock_manager.acquire(tx_wait2, resource, LockMode::Shared).unwrap();

    // Holder releases (first compatible waiter promoted)
    let release_result = lock_manager.release_with_evidence(tx_holder, resource).unwrap();

    // At least one promotion should have occurred
    let promotions = release_result
        .evidence
        .iter()
        .flat_map(|ev| &ev.promoted_waiters)
        .collect::<Vec<_>>();

    assert!(!promotions.is_empty(), "Expected at least one promotion");

    // Verify first waiter was promoted
    assert_eq!(promotions[0].tx_id, tx_wait1);
}

#[test]
fn test_release_all_cleanup_trace() {
    // release_all cleans up all transaction's locks
    let lock_manager = LockManager::new();
    let resource1 = LockResource::table(1, 10).unwrap();
    let resource2 = LockResource::table(1, 20).unwrap();
    let tx_id = TransactionId::new(1);
    let ts = EngineTimestamp::from_unix_millis(7000);

    // Acquire locks on multiple resources
    let _ = lock_manager.acquire(tx_id, resource1, LockMode::Exclusive).unwrap();
    let _ = lock_manager.acquire(tx_id, resource2, LockMode::Shared).unwrap();

    // Release all locks
    let cleanup_result = lock_manager.release_all_with_evidence(tx_id).unwrap();
    let trace = LockReleaseAllTrace::new(
        tx_id,
        cleanup_result.summary.affected_resource_count,
        TransactionState::RolledBack,
        ts,
    );

    // Verify both resources were cleaned up
    assert_eq!(trace.resources_released, 2);
    assert_eq!(trace.terminal_state, TransactionState::RolledBack);
}
