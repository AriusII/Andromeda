use super::fixtures::{assert_granted, holder, row_resource, schema_resource, table_resource};
use super::*;

#[test]
fn waiting_acquisition_projects_critical_wait_evidence() {
    let manager = LockManager::new();
    let resource = row_resource(71, 701);
    let holder_tx = TransactionId::new(71);
    let waiter = TransactionId::new(72);

    assert_granted(&manager, holder_tx, resource, LockMode::Exclusive);

    let result = manager
        .acquire_with_evidence(waiter, resource, LockMode::Shared)
        .unwrap();

    assert!(matches!(
        result.status,
        LockAcquireStatus::Waiting { sequence: 1, .. }
    ));
    let evidence = result.evidence.expect("waiting acquire must be traced");
    assert_eq!(evidence.kind, LockTraceKind::CriticalWait);
    assert_eq!(evidence.tx_id, waiter);
    assert_eq!(evidence.resource, Some(resource));
    assert_eq!(evidence.mode, Some(LockMode::Shared));
    assert_eq!(
        evidence.blockers,
        vec![holder(holder_tx, LockMode::Exclusive)]
    );
    assert_eq!(evidence.outcome, LockTraceOutcome::Waiting);
}

#[test]
fn release_promotion_evidence_identifies_promoted_waiter() {
    let manager = LockManager::new();
    let resource = table_resource(72);
    let holder_tx = TransactionId::new(73);
    let waiter = TransactionId::new(74);

    assert_granted(&manager, holder_tx, resource, LockMode::Exclusive);
    assert!(matches!(
        manager.acquire(waiter, resource, LockMode::Shared).unwrap(),
        LockAcquireStatus::Waiting { sequence: 1, .. }
    ));

    let release = manager.release_with_evidence(holder_tx, resource).unwrap();

    assert!(release.released_any);
    assert_eq!(release.evidence.len(), 1);
    let evidence = &release.evidence[0];
    assert_eq!(evidence.kind, LockTraceKind::Promotion);
    assert_eq!(evidence.tx_id, waiter);
    assert_eq!(evidence.resource, Some(resource));
    assert_eq!(evidence.mode, Some(LockMode::Shared));
    assert_eq!(evidence.outcome, LockTraceOutcome::Promoted);
    assert_eq!(evidence.promoted_waiters.len(), 1);
    assert_eq!(evidence.promoted_waiters[0].tx_id, waiter);
    assert_eq!(evidence.promoted_waiters[0].sequence, 1);
}

#[test]
fn release_all_summary_projects_trace_evidence() {
    let manager = LockManager::new();
    let held_resource = schema_resource(1);
    let waited_resource = row_resource(73, 703);
    let ending_tx = TransactionId::new(75);
    let other_tx = TransactionId::new(76);

    assert_granted(&manager, ending_tx, held_resource, LockMode::SchemaShared);
    assert_granted(&manager, other_tx, waited_resource, LockMode::Exclusive);
    assert!(matches!(
        manager
            .acquire(ending_tx, waited_resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { .. }
    ));

    let release_all = manager.release_all_with_evidence(ending_tx).unwrap();

    assert_eq!(
        release_all.summary,
        LockReleaseAllSummary {
            affected_resource_count: 2,
            released_holder_count: 1,
            removed_waiter_count: 1,
        }
    );
    let trace = release_all.summary.to_trace(ending_tx);
    assert_eq!(trace.kind, LockTraceKind::ReleaseAll);
    assert_eq!(trace.tx_id, ending_tx);
    assert_eq!(trace.resource, None);
    assert_eq!(trace.outcome, LockTraceOutcome::Released);
    assert_eq!(trace.affected_resource_count, 2);
    assert_eq!(release_all.evidence[0], trace);
}
