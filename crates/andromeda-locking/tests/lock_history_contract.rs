//! Contract tests for lock history trace infrastructure owned by locking.

use andromeda_locking::{
    DeadlockAuditTrace, DeadlockDecisionKind, LockAcquireStatus, LockManager, LockMode,
    LockPromotionTrace, LockResource, LockWaitTrace,
};
use andromeda_time::EngineTimestamp;
use andromeda_types::TransactionId;

#[test]
fn wait_trace_captures_contention_blockers() {
    let lock_manager = LockManager::new();
    let resource = LockResource::table(1, 10).unwrap();
    let tx1 = TransactionId::new(1);
    let tx2 = TransactionId::new(2);
    let ts = EngineTimestamp::from_unix_millis(1000);

    assert_eq!(
        lock_manager
            .acquire(tx1, resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    let status = lock_manager
        .acquire(tx2, resource, LockMode::Shared)
        .unwrap();

    match status {
        LockAcquireStatus::Waiting { blockers, .. } => {
            let wait_trace = LockWaitTrace::new(tx2, resource, LockMode::Shared, blockers, ts);

            assert_eq!(wait_trace.tx_id, tx2);
            assert_eq!(wait_trace.resource_id, resource);
            assert_eq!(wait_trace.requested_mode, LockMode::Shared);
            assert_eq!(wait_trace.blocker_tx_ids, vec![tx1]);
            assert_eq!(wait_trace.timestamp, ts);
        },
        other => panic!("expected waiting status, got {other:?}"),
    }
}

#[test]
fn promotion_trace_captures_waiter_promoted_by_release() {
    let lock_manager = LockManager::new();
    let resource = LockResource::table(1, 10).unwrap();
    let tx1 = TransactionId::new(1);
    let tx2 = TransactionId::new(2);
    let ts_promote = EngineTimestamp::from_unix_millis(2001);

    lock_manager
        .acquire(tx1, resource, LockMode::Exclusive)
        .unwrap();
    lock_manager
        .acquire(tx2, resource, LockMode::Shared)
        .unwrap();

    let release_result = lock_manager.release_with_evidence(tx1, resource).unwrap();
    let promoted_waiter = release_result
        .evidence
        .iter()
        .flat_map(|evidence| &evidence.promoted_waiters)
        .next()
        .expect("expected a promoted waiter");
    let promotion_trace = LockPromotionTrace::new(
        promoted_waiter.tx_id,
        resource,
        promoted_waiter.mode,
        tx1,
        ts_promote,
    );

    assert!(release_result.released_any);
    assert_eq!(promotion_trace.tx_id, tx2);
    assert_eq!(promotion_trace.resource_id, resource);
    assert_eq!(promotion_trace.mode, LockMode::Shared);
    assert_eq!(promotion_trace.released_by_tx, tx1);
}

#[test]
fn deadlock_decision_trace_captures_victim_suggestion() {
    let ts = EngineTimestamp::from_unix_millis(3000);
    let cycle_txs = vec![TransactionId::new(1), TransactionId::new(2)];
    let victim_tx = TransactionId::new(1);

    let trace = DeadlockAuditTrace::victim_suggested(
        ts,
        cycle_txs.clone(),
        victim_tx,
        "cycle detected: youngest by start order selected as victim",
    );

    assert_eq!(trace.detection_ts, ts);
    assert_eq!(trace.cycle_txs, cycle_txs);
    assert_eq!(trace.decision, DeadlockDecisionKind::VictimSuggested);
    assert_eq!(trace.victim_tx_id, Some(victim_tx));
    assert!(trace.reason.contains("cycle detected"));
}

#[test]
fn granted_acquire_does_not_create_wait_trace_inputs() {
    let lock_manager = LockManager::new();
    let resource = LockResource::table(1, 10).unwrap();
    let tx = TransactionId::new(1);

    let status = lock_manager
        .acquire(tx, resource, LockMode::Shared)
        .unwrap();

    assert_eq!(status, LockAcquireStatus::Granted);
}

#[test]
fn deadlock_no_cycle_trace_has_no_victim() {
    let ts = EngineTimestamp::from_unix_millis(6000);

    let trace = DeadlockAuditTrace::no_cycle_detected(ts);

    assert_eq!(trace.detection_ts, ts);
    assert!(trace.cycle_txs.is_empty());
    assert_eq!(trace.decision, DeadlockDecisionKind::NoCycleDetected);
    assert_eq!(trace.victim_tx_id, None);
}

#[test]
fn release_can_promote_multiple_compatible_waiters() {
    let lock_manager = LockManager::new();
    let resource = LockResource::table(1, 10).unwrap();
    let tx_holder = TransactionId::new(1);
    let tx_wait1 = TransactionId::new(2);
    let tx_wait2 = TransactionId::new(3);

    lock_manager
        .acquire(tx_holder, resource, LockMode::Exclusive)
        .unwrap();
    lock_manager
        .acquire(tx_wait1, resource, LockMode::Shared)
        .unwrap();
    lock_manager
        .acquire(tx_wait2, resource, LockMode::Shared)
        .unwrap();

    let release_result = lock_manager
        .release_with_evidence(tx_holder, resource)
        .unwrap();
    let promotions = release_result
        .evidence
        .iter()
        .flat_map(|ev| &ev.promoted_waiters)
        .collect::<Vec<_>>();

    assert!(!promotions.is_empty(), "expected at least one promotion");
    assert_eq!(promotions[0].tx_id, tx_wait1);
}

#[test]
fn release_all_cleanup_reports_affected_resources() {
    let lock_manager = LockManager::new();
    let resource1 = LockResource::table(1, 10).unwrap();
    let resource2 = LockResource::table(1, 20).unwrap();
    let tx_id = TransactionId::new(1);

    lock_manager
        .acquire(tx_id, resource1, LockMode::Exclusive)
        .unwrap();
    lock_manager
        .acquire(tx_id, resource2, LockMode::Shared)
        .unwrap();

    let cleanup_result = lock_manager.release_all_with_evidence(tx_id).unwrap();

    assert_eq!(cleanup_result.summary.affected_resource_count, 2);
    assert_eq!(cleanup_result.summary.released_holder_count, 2);
    assert_eq!(cleanup_result.summary.removed_waiter_count, 0);
}

#[test]
fn wait_promotion_and_cleanup_evidence_share_transaction_correlation() {
    let lock_manager = LockManager::new();
    let resource = LockResource::table(1, 10).unwrap();
    let tx_holder = TransactionId::new(1);
    let tx_waiter = TransactionId::new(2);
    let ts_wait = EngineTimestamp::from_unix_millis(5000);
    let ts_promote = EngineTimestamp::from_unix_millis(5100);

    lock_manager
        .acquire(tx_holder, resource, LockMode::Exclusive)
        .unwrap();
    let acquire_status = lock_manager
        .acquire(tx_waiter, resource, LockMode::Shared)
        .unwrap();
    let wait_trace = match acquire_status {
        LockAcquireStatus::Waiting { blockers, .. } => {
            LockWaitTrace::new(tx_waiter, resource, LockMode::Shared, blockers, ts_wait)
        },
        _ => panic!("expected waiting status"),
    };

    let release_result = lock_manager
        .release_with_evidence(tx_holder, resource)
        .unwrap();
    let promoted_waiter = release_result
        .evidence
        .iter()
        .flat_map(|evidence| &evidence.promoted_waiters)
        .next()
        .expect("expected promotion trace inputs");
    let promotion_trace = LockPromotionTrace::new(
        promoted_waiter.tx_id,
        resource,
        promoted_waiter.mode,
        tx_holder,
        ts_promote,
    );

    let cleanup_result = lock_manager.release_all_with_evidence(tx_waiter).unwrap();

    assert_eq!(wait_trace.tx_id, tx_waiter);
    assert_eq!(promotion_trace.tx_id, tx_waiter);
    assert_eq!(cleanup_result.summary.affected_resource_count, 1);
    assert!(wait_trace.timestamp < promotion_trace.timestamp);
    assert_eq!(wait_trace.resource_id, resource);
    assert_eq!(promotion_trace.resource_id, resource);
    assert_eq!(promotion_trace.released_by_tx, tx_holder);
}
