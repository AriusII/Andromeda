use super::helpers::{
    assert_cycle, detector_with_policy, graph_with_edges, metadata_table,
    two_resource_deadlock_fixture,
};
use super::*;

#[test]
fn detector_reports_no_cycle_for_empty_graph() {
    let detector = DeadlockDetector::new(DeadlockPolicy::default()).unwrap();
    let graph = WaitForGraph::new();

    assert_eq!(
        detector.detect(&graph).unwrap(),
        DeadlockDetectionStatus::NoCycle
    );
}

#[test]
fn deadline_aware_detection_before_deadline_finds_cycle() {
    let detector = detector_with_policy(DeadlockVictimPolicy::YoungestTransactionId);
    let mut clock = ManualDeadlockClock::new();
    let deadline = DeadlockDetectionDeadline::from_policy(detector.policy(), &clock).unwrap();
    let graph = graph_with_edges(&[(1, 2), (2, 1)]);

    let edges_before = graph.edges();
    clock.advance(Duration::from_millis(499)).unwrap();

    assert_cycle(
        detector
            .detect_with_deadline(&graph, deadline, &clock)
            .unwrap(),
        vec![TransactionId::new(1), TransactionId::new(2)],
        TransactionId::new(2),
        DeadlockVictimPolicy::YoungestTransactionId,
    );
    assert_eq!(graph.edges(), edges_before);
}

#[test]
fn expired_deadline_returns_explicit_timeout_status_without_graph_mutation() {
    let detector = detector_with_policy(DeadlockVictimPolicy::YoungestTransactionId);
    let mut clock = ManualDeadlockClock::new();
    let deadline = DeadlockDetectionDeadline::from_policy(detector.policy(), &clock).unwrap();
    let graph = graph_with_edges(&[(1, 2), (2, 1)]);

    let edges_before = graph.edges();
    clock.advance(Duration::from_millis(500)).unwrap();

    assert_eq!(
        detector
            .detect_with_deadline(&graph, deadline, &clock)
            .unwrap(),
        DeadlockDetectionStatus::TimedOut {
            reason: DEADLOCK_DETECTION_TIMEOUT_REASON,
            elapsed: Duration::from_millis(500),
            timeout: Duration::from_millis(500),
        }
    );
    assert_eq!(graph.edges(), edges_before);
}

#[test]
fn detector_reports_no_cycle_for_non_empty_acyclic_graph() {
    let detector = DeadlockDetector::default();
    let graph = graph_with_edges(&[(1, 2), (2, 3)]);

    assert_eq!(
        detector.detect(&graph).unwrap(),
        DeadlockDetectionStatus::NoCycle
    );
}

#[test]
fn detector_finds_simple_two_node_cycle() {
    let detector = DeadlockDetector::default();
    let graph = graph_with_edges(&[(1, 2), (2, 1)]);
    let metadata = metadata_table(&[(1, 1), (2, 2)]);

    assert_cycle(
        detector
            .detect_with_transaction_metadata(&graph, &metadata)
            .unwrap(),
        vec![TransactionId::new(1), TransactionId::new(2)],
        TransactionId::new(2),
        DeadlockVictimPolicy::YoungestTransactionStartOrder,
    );
}

#[test]
fn detector_finds_three_node_cycle() {
    let detector = DeadlockDetector::default();
    let graph = graph_with_edges(&[(3, 1), (1, 2), (2, 3)]);
    let metadata = metadata_table(&[(1, 1), (2, 2), (3, 3)]);

    assert_cycle(
        detector
            .detect_with_transaction_metadata(&graph, &metadata)
            .unwrap(),
        vec![
            TransactionId::new(1),
            TransactionId::new(2),
            TransactionId::new(3),
        ],
        TransactionId::new(3),
        DeadlockVictimPolicy::YoungestTransactionStartOrder,
    );
}

#[test]
fn detector_selects_youngest_transaction_by_start_order_in_cycle() {
    let detector = DeadlockDetector::default();
    let graph = graph_with_edges(&[(20, 10), (10, 20)]);
    let metadata = metadata_table(&[(10, 3), (20, 1)]);

    assert_cycle(
        detector
            .detect_with_transaction_metadata(&graph, &metadata)
            .unwrap(),
        vec![TransactionId::new(10), TransactionId::new(20)],
        TransactionId::new(10),
        DeadlockVictimPolicy::YoungestTransactionStartOrder,
    );
}

#[test]
fn detector_ties_youngest_transaction_policy_by_tx_id_deterministically() {
    let detector = DeadlockDetector::default();
    let graph = graph_with_edges(&[(10, 20), (20, 30), (30, 10)]);
    let metadata = metadata_table(&[(10, 7), (20, 7), (30, 7)]);

    assert_cycle(
        detector
            .detect_with_transaction_metadata(&graph, &metadata)
            .unwrap(),
        vec![
            TransactionId::new(10),
            TransactionId::new(20),
            TransactionId::new(30),
        ],
        TransactionId::new(30),
        DeadlockVictimPolicy::YoungestTransactionStartOrder,
    );
}

#[test]
fn detector_reports_deferred_when_youngest_metadata_is_missing() {
    let detector = DeadlockDetector::default();
    let graph = graph_with_edges(&[(10, 20), (20, 10)]);
    let metadata = metadata_table(&[(10, 1)]);

    assert_eq!(
        detector
            .detect_with_transaction_metadata(&graph, &metadata)
            .unwrap(),
        DeadlockDetectionStatus::Deferred {
            reason: MISSING_TRANSACTION_ORDERING_METADATA_REASON
        }
    );
    assert_eq!(
        detector.detect(&graph).unwrap(),
        DeadlockDetectionStatus::Deferred {
            reason: MISSING_TRANSACTION_ORDERING_METADATA_REASON
        }
    );
}

#[test]
fn detector_still_supports_deterministic_id_based_and_oldest_victims() {
    let graph = graph_with_edges(&[(20, 10), (10, 20)]);

    let youngest_detector = detector_with_policy(DeadlockVictimPolicy::YoungestTransactionId);
    let oldest_detector = detector_with_policy(DeadlockVictimPolicy::OldestTransactionId);

    assert_cycle(
        youngest_detector.detect(&graph).unwrap(),
        vec![TransactionId::new(10), TransactionId::new(20)],
        TransactionId::new(20),
        DeadlockVictimPolicy::YoungestTransactionId,
    );
    assert_cycle(
        oldest_detector.detect(&graph).unwrap(),
        vec![TransactionId::new(10), TransactionId::new(20)],
        TransactionId::new(10),
        DeadlockVictimPolicy::OldestTransactionId,
    );
}

#[test]
fn detector_reports_deterministic_first_cycle_when_multiple_cycles_exist() {
    let detector = DeadlockDetector::default();
    let graph = graph_with_edges(&[(5, 4), (1, 3), (1, 2), (3, 1), (4, 5), (2, 1)]);
    let metadata = metadata_table(&[(1, 1), (2, 2), (3, 3), (4, 4), (5, 5)]);

    assert_cycle(
        detector
            .detect_with_transaction_metadata(&graph, &metadata)
            .unwrap(),
        vec![TransactionId::new(1), TransactionId::new(2)],
        TransactionId::new(2),
        DeadlockVictimPolicy::YoungestTransactionStartOrder,
    );
}

#[test]
fn detector_finds_cycle_derived_from_lock_manager_snapshot() {
    let detector = DeadlockDetector::default();
    let fixture = two_resource_deadlock_fixture(&[(1, 1), (2, 2)]);

    let graph = WaitForGraph::from_lock_manager(&fixture.manager).unwrap();

    assert_eq!(
        graph.edges(),
        vec![
            (fixture.first_tx, fixture.second_tx),
            (fixture.second_tx, fixture.first_tx),
        ]
    );
    assert_cycle(
        detector
            .detect_with_transaction_metadata(&graph, &fixture.metadata)
            .unwrap(),
        vec![fixture.first_tx, fixture.second_tx],
        fixture.second_tx,
        DeadlockVictimPolicy::YoungestTransactionStartOrder,
    );
}

#[test]
fn youngest_policy_does_not_mutate_lock_manager_graph_or_metadata() {
    let detector = DeadlockDetector::default();
    let fixture = two_resource_deadlock_fixture(&[(1, 2), (2, 1)]);
    let transaction_statuses = TransactionStatusTable::new();
    transaction_statuses
        .record(fixture.first_tx, TransactionStatus::InFlight)
        .unwrap();
    transaction_statuses
        .record(fixture.second_tx, TransactionStatus::InFlight)
        .unwrap();

    let snapshot_before = fixture.manager.snapshot().unwrap();
    let graph = WaitForGraph::from_lock_manager(&fixture.manager).unwrap();
    let graph_before = graph.clone();
    let metadata_before = fixture.metadata.clone();
    let first_status_before = transaction_statuses.status(fixture.first_tx);
    let second_status_before = transaction_statuses.status(fixture.second_tx);

    assert_cycle(
        detector
            .detect_with_transaction_metadata(&graph, &fixture.metadata)
            .unwrap(),
        vec![fixture.first_tx, fixture.second_tx],
        fixture.first_tx,
        DeadlockVictimPolicy::YoungestTransactionStartOrder,
    );

    assert_eq!(fixture.manager.snapshot().unwrap(), snapshot_before);
    assert_eq!(graph, graph_before);
    assert_eq!(fixture.metadata, metadata_before);
    assert_eq!(
        transaction_statuses.status(fixture.first_tx),
        first_status_before
    );
    assert_eq!(
        transaction_statuses.status(fixture.second_tx),
        second_status_before
    );
}
