//! Contract tests for transaction deadlock detection.

use std::time::Duration;

use andromeda_core::TransactionId;
use andromeda_tx::deadlock_detection::{
    DeadlockClock, DeadlockDetectionDeadline, DeadlockDetectionStatus, DeadlockDetector,
    DeadlockPolicy, DeadlockTransactionMetadataTable, DeadlockVictimPolicy, ManualDeadlockClock,
    WaitForGraph,
};

fn tx(id: u64) -> TransactionId {
    TransactionId::new(id)
}

fn detector(victim_policy: DeadlockVictimPolicy) -> DeadlockDetector {
    let policy = DeadlockPolicy::new(Duration::from_millis(500), victim_policy).unwrap();
    DeadlockDetector::new(policy).unwrap()
}

fn cycle_status(
    graph: &WaitForGraph,
    victim_policy: DeadlockVictimPolicy,
) -> DeadlockDetectionStatus {
    detector(victim_policy).detect(graph).unwrap()
}

#[test]
fn empty_graph_reports_no_cycle() {
    let graph = WaitForGraph::new();

    assert_eq!(
        cycle_status(&graph, DeadlockVictimPolicy::OldestTransactionId),
        DeadlockDetectionStatus::NoCycle
    );
}

#[test]
fn linear_wait_chain_reports_no_cycle() {
    let mut graph = WaitForGraph::new();
    graph.insert_edge(tx(1), tx(2)).unwrap();
    graph.insert_edge(tx(2), tx(3)).unwrap();
    graph.insert_edge(tx(3), tx(4)).unwrap();

    assert_eq!(
        cycle_status(&graph, DeadlockVictimPolicy::OldestTransactionId),
        DeadlockDetectionStatus::NoCycle
    );
}

#[test]
fn simple_cycle_reports_cycle_and_oldest_victim() {
    let mut graph = WaitForGraph::new();
    graph.insert_edge(tx(5), tx(10)).unwrap();
    graph.insert_edge(tx(10), tx(5)).unwrap();

    let status = cycle_status(&graph, DeadlockVictimPolicy::OldestTransactionId);

    let DeadlockDetectionStatus::CycleFound {
        victim,
        cycle_participants,
    } = status
    else {
        panic!("expected cycle");
    };

    assert_eq!(victim.tx_id, tx(5));
    assert_eq!(cycle_participants, vec![tx(5), tx(10)]);
}

#[test]
fn simple_cycle_reports_youngest_transaction_id_victim() {
    let mut graph = WaitForGraph::new();
    graph.insert_edge(tx(5), tx(10)).unwrap();
    graph.insert_edge(tx(10), tx(5)).unwrap();

    let status = cycle_status(&graph, DeadlockVictimPolicy::YoungestTransactionId);

    let DeadlockDetectionStatus::CycleFound { victim, .. } = status else {
        panic!("expected cycle");
    };

    assert_eq!(victim.tx_id, tx(10));
}

#[test]
fn start_order_policy_uses_registered_transaction_metadata() {
    let mut graph = WaitForGraph::new();
    graph.insert_edge(tx(1), tx(2)).unwrap();
    graph.insert_edge(tx(2), tx(3)).unwrap();
    graph.insert_edge(tx(3), tx(1)).unwrap();

    let mut metadata = DeadlockTransactionMetadataTable::new();
    metadata.register_start_order(tx(1), 10).unwrap();
    metadata.register_start_order(tx(2), 30).unwrap();
    metadata.register_start_order(tx(3), 20).unwrap();

    let status = detector(DeadlockVictimPolicy::YoungestTransactionStartOrder)
        .detect_with_transaction_metadata(&graph, &metadata)
        .unwrap();

    let DeadlockDetectionStatus::CycleFound { victim, .. } = status else {
        panic!("expected cycle");
    };

    assert_eq!(victim.tx_id, tx(2));
}

#[test]
fn missing_start_order_metadata_defers_detection() {
    let mut graph = WaitForGraph::new();
    graph.insert_edge(tx(1), tx(2)).unwrap();
    graph.insert_edge(tx(2), tx(1)).unwrap();

    let metadata = DeadlockTransactionMetadataTable::new();

    let status = detector(DeadlockVictimPolicy::YoungestTransactionStartOrder)
        .detect_with_transaction_metadata(&graph, &metadata)
        .unwrap();

    assert!(matches!(status, DeadlockDetectionStatus::Deferred { .. }));
}

#[test]
fn edge_storage_is_deterministic() {
    let mut graph = WaitForGraph::new();
    graph.insert_edge(tx(5), tx(2)).unwrap();
    graph.insert_edge(tx(2), tx(8)).unwrap();
    graph.insert_edge(tx(8), tx(1)).unwrap();
    graph.insert_edge(tx(1), tx(5)).unwrap();

    assert_eq!(
        graph.edges(),
        vec![
            (tx(1), tx(5)),
            (tx(2), tx(8)),
            (tx(5), tx(2)),
            (tx(8), tx(1))
        ]
    );
}

#[test]
fn removing_transaction_cleans_incoming_and_outgoing_edges() {
    let mut graph = WaitForGraph::new();
    graph.insert_edge(tx(1), tx(2)).unwrap();
    graph.insert_edge(tx(2), tx(3)).unwrap();
    graph.insert_edge(tx(3), tx(2)).unwrap();

    assert!(graph.remove_transaction(tx(2)).unwrap());

    assert!(graph.is_empty());
}

#[test]
fn invalid_edges_are_rejected_without_mutating_graph() {
    let mut graph = WaitForGraph::new();

    assert!(graph.insert_edge(tx(0), tx(1)).is_err());
    assert!(graph.insert_edge(tx(1), tx(1)).is_err());
    assert!(graph.is_empty());
}

#[test]
fn duplicate_edge_is_idempotent() {
    let mut graph = WaitForGraph::new();

    assert!(graph.insert_edge(tx(1), tx(2)).unwrap());
    assert!(!graph.insert_edge(tx(1), tx(2)).unwrap());
    assert_eq!(graph.edge_count(), 1);
}

#[test]
fn deadline_reports_timeout_before_graph_walk() {
    let mut graph = WaitForGraph::new();
    graph.insert_edge(tx(1), tx(2)).unwrap();
    graph.insert_edge(tx(2), tx(1)).unwrap();

    let started_clock = ManualDeadlockClock::new();
    let deadline =
        DeadlockDetectionDeadline::new(started_clock.now(), Duration::from_millis(10)).unwrap();
    let expired_clock = ManualDeadlockClock::at(Duration::from_millis(10));

    let status = detector(DeadlockVictimPolicy::OldestTransactionId)
        .detect_with_deadline(&graph, deadline, &expired_clock)
        .unwrap();

    assert!(matches!(status, DeadlockDetectionStatus::TimedOut { .. }));
}

#[test]
fn timeout_policy_bounds_are_enforced() {
    assert!(
        DeadlockPolicy::new(Duration::ZERO, DeadlockVictimPolicy::OldestTransactionId).is_err()
    );
    assert!(
        DeadlockPolicy::new(
            Duration::from_secs(61),
            DeadlockVictimPolicy::OldestTransactionId
        )
        .is_err()
    );
}
