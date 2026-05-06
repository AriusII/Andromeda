use super::*;
use crate::lock_manager::{LockAcquireStatus, LockMode, LockResource};
use crate::{TransactionStatus, TransactionStatusTable};

#[test]
fn graph_edge_insertion_is_deterministic_and_idempotent() {
    let mut graph = WaitForGraph::new();

    assert!(
        graph
            .insert_edge(TransactionId::new(3), TransactionId::new(2))
            .unwrap()
    );
    assert!(
        graph
            .insert_edge(TransactionId::new(1), TransactionId::new(4))
            .unwrap()
    );
    assert!(
        graph
            .insert_edge(TransactionId::new(1), TransactionId::new(2))
            .unwrap()
    );
    assert!(
        !graph
            .insert_edge(TransactionId::new(1), TransactionId::new(2))
            .unwrap()
    );

    assert_eq!(
        graph.edges(),
        vec![
            (TransactionId::new(1), TransactionId::new(2)),
            (TransactionId::new(1), TransactionId::new(4)),
            (TransactionId::new(3), TransactionId::new(2)),
        ]
    );
    assert_eq!(graph.edge_count(), 3);
}

#[test]
fn graph_from_lock_manager_adds_exclusive_waiter_edge_to_shared_holder() {
    let manager = LockManager::new();
    let resource = LockResource::row(1, 2, 3).unwrap();

    assert_eq!(
        manager
            .acquire(TransactionId::new(1), resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert!(matches!(
        manager
            .acquire(TransactionId::new(2), resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Waiting { sequence: 1, .. }
    ));

    let graph = WaitForGraph::from_lock_manager(&manager).unwrap();

    assert_eq!(
        graph.edges(),
        vec![(TransactionId::new(2), TransactionId::new(1))]
    );
}

#[test]
fn graph_from_lock_manager_omits_edge_for_compatible_waiter() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();

    manager
        .record_holder(resource, TransactionId::new(1), LockMode::Shared)
        .unwrap();
    manager
        .enqueue_waiter(resource, TransactionId::new(2), LockMode::Shared)
        .unwrap();

    let graph = WaitForGraph::from_lock_manager(&manager).unwrap();

    assert!(graph.is_empty());
}

#[test]
fn graph_from_lock_manager_upgrade_waiter_excludes_self_holder() {
    let manager = LockManager::new();
    let resource = LockResource::row(1, 2, 3).unwrap();
    let upgrading_tx = TransactionId::new(7);
    let blocking_tx = TransactionId::new(8);

    assert_eq!(
        manager
            .acquire(upgrading_tx, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(blocking_tx, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert!(matches!(
        manager
            .acquire(upgrading_tx, resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::WaitingUpgrade { sequence: 1, .. }
    ));

    let graph = WaitForGraph::from_lock_manager(&manager).unwrap();

    assert_eq!(graph.edges(), vec![(upgrading_tx, blocking_tx)]);
}

#[test]
fn graph_from_lock_manager_combines_multi_resource_edges_deterministically() {
    let manager = LockManager::new();
    let first_resource = LockResource::table(1, 2).unwrap();
    let second_resource = LockResource::row(1, 2, 3).unwrap();

    assert_eq!(
        manager
            .acquire(TransactionId::new(1), first_resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert!(matches!(
        manager
            .acquire(TransactionId::new(2), first_resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { sequence: 1, .. }
    ));
    assert_eq!(
        manager
            .acquire(TransactionId::new(3), second_resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(TransactionId::new(5), second_resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert!(matches!(
        manager
            .acquire(TransactionId::new(4), second_resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Waiting { sequence: 2, .. }
    ));

    let graph = WaitForGraph::from_lock_manager(&manager).unwrap();

    assert_eq!(
        graph.edges(),
        vec![
            (TransactionId::new(2), TransactionId::new(1)),
            (TransactionId::new(4), TransactionId::new(3)),
            (TransactionId::new(4), TransactionId::new(5)),
        ]
    );
}

#[test]
fn graph_from_empty_lock_manager_is_empty() {
    let manager = LockManager::new();

    let graph = WaitForGraph::from_lock_manager(&manager).unwrap();

    assert!(graph.is_empty());
    assert_eq!(graph.edge_count(), 0);
}

#[test]
fn graph_edge_removal_removes_empty_waiter_entries() {
    let mut graph = WaitForGraph::new();

    graph
        .insert_edge(TransactionId::new(1), TransactionId::new(2))
        .unwrap();
    graph
        .insert_edge(TransactionId::new(1), TransactionId::new(3))
        .unwrap();

    assert!(
        graph
            .remove_edge(TransactionId::new(1), TransactionId::new(2))
            .unwrap()
    );
    assert_eq!(
        graph.blockers_for(TransactionId::new(1)).unwrap(),
        vec![TransactionId::new(3)]
    );
    assert!(
        graph
            .remove_edge(TransactionId::new(1), TransactionId::new(3))
            .unwrap()
    );
    assert!(graph.is_empty());
    assert!(
        !graph
            .remove_edge(TransactionId::new(1), TransactionId::new(3))
            .unwrap()
    );
}

#[test]
fn graph_remove_transaction_removes_incoming_and_outgoing_edges() {
    let mut graph = WaitForGraph::new();

    graph
        .insert_edge(TransactionId::new(1), TransactionId::new(2))
        .unwrap();
    graph
        .insert_edge(TransactionId::new(2), TransactionId::new(3))
        .unwrap();
    graph
        .insert_edge(TransactionId::new(4), TransactionId::new(2))
        .unwrap();

    assert!(graph.remove_transaction(TransactionId::new(2)).unwrap());
    assert_eq!(graph.edges(), Vec::<(TransactionId, TransactionId)>::new());
}

#[test]
fn zero_transaction_ids_are_rejected() {
    let mut graph = WaitForGraph::new();

    assert!(
        graph
            .insert_edge(TransactionId::new(0), TransactionId::new(1))
            .is_err()
    );
    assert!(
        graph
            .insert_edge(TransactionId::new(1), TransactionId::new(0))
            .is_err()
    );
    assert!(graph.remove_transaction(TransactionId::new(0)).is_err());
    assert!(
        DeadlockVictim::from_cycle(
            vec![TransactionId::new(1), TransactionId::new(0)],
            DeadlockVictimPolicy::YoungestTransactionId,
        )
        .is_err()
    );
}

#[test]
fn victim_construction_is_deterministic() {
    let victim = DeadlockVictim::from_cycle(
        vec![
            TransactionId::new(5),
            TransactionId::new(2),
            TransactionId::new(9),
            TransactionId::new(5),
        ],
        DeadlockVictimPolicy::YoungestTransactionId,
    )
    .unwrap();

    assert_eq!(victim.tx_id, TransactionId::new(9));
    assert_eq!(
        victim.cycle_participants,
        vec![
            TransactionId::new(2),
            TransactionId::new(5),
            TransactionId::new(9),
        ]
    );

    let oldest = DeadlockVictim::from_cycle(
        vec![TransactionId::new(5), TransactionId::new(2)],
        DeadlockVictimPolicy::OldestTransactionId,
    )
    .unwrap();
    assert_eq!(oldest.tx_id, TransactionId::new(2));

    let mut metadata = DeadlockTransactionMetadataTable::new();
    metadata
        .register_start_order(TransactionId::new(5), 1)
        .unwrap();
    metadata
        .register_start_order(TransactionId::new(2), 3)
        .unwrap();
    let youngest = DeadlockVictim::from_cycle_with_transaction_metadata(
        vec![TransactionId::new(5), TransactionId::new(2)],
        DeadlockVictimPolicy::YoungestTransactionStartOrder,
        &metadata,
    )
    .unwrap();
    assert_eq!(youngest.tx_id, TransactionId::new(2));
}

#[test]
fn default_policy_is_valid_and_timeout_is_bounded() {
    assert!(DeadlockPolicy::default().validate().is_ok());
    assert_eq!(DEFAULT_DEADLOCK_TIMEOUT, Duration::from_millis(500));
    assert_eq!(
        DeadlockPolicy::default().detection_timeout,
        Duration::from_millis(500)
    );
    assert!(
        DeadlockPolicy::new(
            Duration::from_millis(500),
            DeadlockVictimPolicy::YoungestTransactionId,
        )
        .is_ok()
    );
    assert!(
        DeadlockPolicy::new(Duration::ZERO, DeadlockVictimPolicy::YoungestTransactionId).is_err()
    );
    assert!(
        DeadlockPolicy::new(
            MAX_DEADLOCK_TIMEOUT + Duration::from_nanos(1),
            DeadlockVictimPolicy::YoungestTransactionId,
        )
        .is_err()
    );
}

#[test]
fn deadline_rejects_zero_and_too_large_timeouts() {
    let clock = ManualDeadlockClock::new();

    assert!(DeadlockDetectionDeadline::new(clock.now(), Duration::ZERO).is_err());
    assert!(DeadlockDetectionDeadline::new(
        clock.now(),
        MAX_DEADLOCK_TIMEOUT + Duration::from_nanos(1),
    )
    .is_err());
}

#[test]
fn manual_deadlock_clock_is_deterministic() {
    let mut clock = ManualDeadlockClock::at(Duration::from_secs(2));
    let policy = DeadlockPolicy::new(
        Duration::from_millis(500),
        DeadlockVictimPolicy::YoungestTransactionId,
    )
    .unwrap();
    let deadline = DeadlockDetectionDeadline::from_policy(policy, &clock).unwrap();

    assert_eq!(
        deadline.started_at().elapsed_since_clock_start(),
        Duration::from_secs(2)
    );
    assert!(!deadline.is_expired(&clock));

    clock.advance(Duration::from_millis(499)).unwrap();
    assert!(!deadline.is_expired(&clock));

    clock.advance(Duration::from_millis(1)).unwrap();
    assert!(deadline.is_expired(&clock));
}

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
    let detector = DeadlockDetector::new(
        DeadlockPolicy::new(
            Duration::from_millis(500),
            DeadlockVictimPolicy::YoungestTransactionId,
        )
        .unwrap(),
    )
    .unwrap();
    let mut clock = ManualDeadlockClock::new();
    let deadline = DeadlockDetectionDeadline::from_policy(detector.policy(), &clock).unwrap();
    let mut graph = WaitForGraph::new();

    graph
        .insert_edge(TransactionId::new(1), TransactionId::new(2))
        .unwrap();
    graph
        .insert_edge(TransactionId::new(2), TransactionId::new(1))
        .unwrap();
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
    let detector = DeadlockDetector::new(
        DeadlockPolicy::new(
            Duration::from_millis(500),
            DeadlockVictimPolicy::YoungestTransactionId,
        )
        .unwrap(),
    )
    .unwrap();
    let mut clock = ManualDeadlockClock::new();
    let deadline = DeadlockDetectionDeadline::from_policy(detector.policy(), &clock).unwrap();
    let mut graph = WaitForGraph::new();

    graph
        .insert_edge(TransactionId::new(1), TransactionId::new(2))
        .unwrap();
    graph
        .insert_edge(TransactionId::new(2), TransactionId::new(1))
        .unwrap();
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
    let mut graph = WaitForGraph::new();

    graph
        .insert_edge(TransactionId::new(1), TransactionId::new(2))
        .unwrap();
    graph
        .insert_edge(TransactionId::new(2), TransactionId::new(3))
        .unwrap();

    assert_eq!(
        detector.detect(&graph).unwrap(),
        DeadlockDetectionStatus::NoCycle
    );
}

#[test]
fn detector_finds_simple_two_node_cycle() {
    let detector = DeadlockDetector::default();
    let mut graph = WaitForGraph::new();
    let metadata = metadata_table(&[(1, 1), (2, 2)]);

    graph
        .insert_edge(TransactionId::new(1), TransactionId::new(2))
        .unwrap();
    graph
        .insert_edge(TransactionId::new(2), TransactionId::new(1))
        .unwrap();

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
    let mut graph = WaitForGraph::new();
    let metadata = metadata_table(&[(1, 1), (2, 2), (3, 3)]);

    graph
        .insert_edge(TransactionId::new(3), TransactionId::new(1))
        .unwrap();
    graph
        .insert_edge(TransactionId::new(1), TransactionId::new(2))
        .unwrap();
    graph
        .insert_edge(TransactionId::new(2), TransactionId::new(3))
        .unwrap();

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
    let mut graph = WaitForGraph::new();
    let metadata = metadata_table(&[(10, 3), (20, 1)]);

    graph
        .insert_edge(TransactionId::new(20), TransactionId::new(10))
        .unwrap();
    graph
        .insert_edge(TransactionId::new(10), TransactionId::new(20))
        .unwrap();

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
    let mut graph = WaitForGraph::new();
    let metadata = metadata_table(&[(10, 7), (20, 7), (30, 7)]);

    graph
        .insert_edge(TransactionId::new(10), TransactionId::new(20))
        .unwrap();
    graph
        .insert_edge(TransactionId::new(20), TransactionId::new(30))
        .unwrap();
    graph
        .insert_edge(TransactionId::new(30), TransactionId::new(10))
        .unwrap();

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
    let mut graph = WaitForGraph::new();
    let metadata = metadata_table(&[(10, 1)]);

    graph
        .insert_edge(TransactionId::new(10), TransactionId::new(20))
        .unwrap();
    graph
        .insert_edge(TransactionId::new(20), TransactionId::new(10))
        .unwrap();

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
    let mut graph = WaitForGraph::new();

    graph
        .insert_edge(TransactionId::new(20), TransactionId::new(10))
        .unwrap();
    graph
        .insert_edge(TransactionId::new(10), TransactionId::new(20))
        .unwrap();

    let youngest_detector = DeadlockDetector::new(
        DeadlockPolicy::new(
            DEFAULT_DEADLOCK_TIMEOUT,
            DeadlockVictimPolicy::YoungestTransactionId,
        )
        .unwrap(),
    )
    .unwrap();
    let oldest_detector = DeadlockDetector::new(
        DeadlockPolicy::new(
            DEFAULT_DEADLOCK_TIMEOUT,
            DeadlockVictimPolicy::OldestTransactionId,
        )
        .unwrap(),
    )
    .unwrap();

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
    let mut graph = WaitForGraph::new();
    let metadata = metadata_table(&[(1, 1), (2, 2), (3, 3), (4, 4), (5, 5)]);

    graph
        .insert_edge(TransactionId::new(5), TransactionId::new(4))
        .unwrap();
    graph
        .insert_edge(TransactionId::new(1), TransactionId::new(3))
        .unwrap();
    graph
        .insert_edge(TransactionId::new(1), TransactionId::new(2))
        .unwrap();
    graph
        .insert_edge(TransactionId::new(3), TransactionId::new(1))
        .unwrap();
    graph
        .insert_edge(TransactionId::new(4), TransactionId::new(5))
        .unwrap();
    graph
        .insert_edge(TransactionId::new(2), TransactionId::new(1))
        .unwrap();

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
    let manager = LockManager::new();
    let first_resource = LockResource::row(1, 2, 3).unwrap();
    let second_resource = LockResource::row(1, 2, 4).unwrap();
    let first_tx = TransactionId::new(1);
    let second_tx = TransactionId::new(2);
    let metadata = metadata_table(&[(1, 1), (2, 2)]);

    assert_eq!(
        manager
            .acquire(first_tx, first_resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(second_tx, second_resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert!(matches!(
        manager
            .acquire(second_tx, first_resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { sequence: 1, .. }
    ));
    assert!(matches!(
        manager
            .acquire(first_tx, second_resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { sequence: 2, .. }
    ));

    let graph = WaitForGraph::from_lock_manager(&manager).unwrap();

    assert_eq!(
        graph.edges(),
        vec![(first_tx, second_tx), (second_tx, first_tx)]
    );
    assert_cycle(
        detector
            .detect_with_transaction_metadata(&graph, &metadata)
            .unwrap(),
        vec![first_tx, second_tx],
        second_tx,
        DeadlockVictimPolicy::YoungestTransactionStartOrder,
    );
}

#[test]
fn youngest_policy_does_not_mutate_lock_manager_graph_or_metadata() {
    let detector = DeadlockDetector::default();
    let manager = LockManager::new();
    let first_resource = LockResource::row(1, 2, 3).unwrap();
    let second_resource = LockResource::row(1, 2, 4).unwrap();
    let first_tx = TransactionId::new(1);
    let second_tx = TransactionId::new(2);
    let metadata = metadata_table(&[(1, 2), (2, 1)]);
    let transaction_statuses = TransactionStatusTable::new();
    transaction_statuses
        .record(first_tx, TransactionStatus::InFlight)
        .unwrap();
    transaction_statuses
        .record(second_tx, TransactionStatus::InFlight)
        .unwrap();

    manager
        .acquire(first_tx, first_resource, LockMode::Exclusive)
        .unwrap();
    manager
        .acquire(second_tx, second_resource, LockMode::Exclusive)
        .unwrap();
    manager
        .acquire(second_tx, first_resource, LockMode::Shared)
        .unwrap();
    manager
        .acquire(first_tx, second_resource, LockMode::Shared)
        .unwrap();

    let snapshot_before = manager.snapshot().unwrap();
    let graph = WaitForGraph::from_lock_manager(&manager).unwrap();
    let graph_before = graph.clone();
    let metadata_before = metadata.clone();
    let first_status_before = transaction_statuses.status(first_tx);
    let second_status_before = transaction_statuses.status(second_tx);

    assert_cycle(
        detector
            .detect_with_transaction_metadata(&graph, &metadata)
            .unwrap(),
        vec![first_tx, second_tx],
        first_tx,
        DeadlockVictimPolicy::YoungestTransactionStartOrder,
    );

    assert_eq!(manager.snapshot().unwrap(), snapshot_before);
    assert_eq!(graph, graph_before);
    assert_eq!(metadata, metadata_before);
    assert_eq!(transaction_statuses.status(first_tx), first_status_before);
    assert_eq!(transaction_statuses.status(second_tx), second_status_before);
}

#[test]
fn decision_from_waiting_lock_cycle_suggests_victim_with_snapshot_evidence() {
    let manager = LockManager::new();
    let first_resource = LockResource::row(1, 2, 3).unwrap();
    let second_resource = LockResource::row(1, 2, 4).unwrap();
    let first_tx = TransactionId::new(1);
    let second_tx = TransactionId::new(2);
    let metadata = metadata_table(&[(1, 1), (2, 2)]);

    manager
        .acquire(first_tx, first_resource, LockMode::Exclusive)
        .unwrap();
    manager
        .acquire(second_tx, second_resource, LockMode::Exclusive)
        .unwrap();
    manager
        .acquire(second_tx, first_resource, LockMode::Shared)
        .unwrap();
    manager
        .acquire(first_tx, second_resource, LockMode::Shared)
        .unwrap();
    let snapshot_before = manager.snapshot().unwrap();

    let decision =
        decide_deadlock_from_lock_manager(&manager, DeadlockPolicy::default(), Some(&metadata))
            .unwrap();

    let DeadlockDecision::VictimSuggested {
        victim,
        cycle_participants,
        evidence,
    } = decision
    else {
        panic!("expected victim suggestion decision");
    };
    assert_eq!(victim.tx_id, second_tx);
    assert_eq!(
        victim.victim_policy,
        DeadlockVictimPolicy::YoungestTransactionStartOrder
    );
    assert_eq!(cycle_participants, vec![first_tx, second_tx]);
    assert_eq!(evidence.lock_snapshot_resource_count, 2);
    assert_eq!(
        evidence.wait_for_edges,
        vec![(first_tx, second_tx), (second_tx, first_tx)]
    );
    assert_eq!(manager.snapshot().unwrap(), snapshot_before);
}

#[test]
fn decision_from_acyclic_waiting_locks_returns_no_cycle() {
    let manager = LockManager::new();
    let resource = LockResource::row(1, 2, 3).unwrap();
    let holder = TransactionId::new(1);
    let waiter = TransactionId::new(2);

    manager
        .acquire(holder, resource, LockMode::Exclusive)
        .unwrap();
    manager.acquire(waiter, resource, LockMode::Shared).unwrap();

    let decision = decide_deadlock_from_lock_manager(
        &manager,
        DeadlockPolicy::new(
            DEFAULT_DEADLOCK_TIMEOUT,
            DeadlockVictimPolicy::YoungestTransactionId,
        )
        .unwrap(),
        None,
    )
    .unwrap();

    let DeadlockDecision::NoCycle { evidence } = decision else {
        panic!("expected no-cycle decision");
    };
    assert_eq!(evidence.lock_snapshot_resource_count, 1);
    assert_eq!(evidence.wait_for_edges, vec![(waiter, holder)]);
}

#[test]
fn decision_defers_when_youngest_policy_metadata_is_missing() {
    let manager = LockManager::new();
    let first_resource = LockResource::row(1, 2, 3).unwrap();
    let second_resource = LockResource::row(1, 2, 4).unwrap();
    let first_tx = TransactionId::new(1);
    let second_tx = TransactionId::new(2);

    manager
        .acquire(first_tx, first_resource, LockMode::Exclusive)
        .unwrap();
    manager
        .acquire(second_tx, second_resource, LockMode::Exclusive)
        .unwrap();
    manager
        .acquire(second_tx, first_resource, LockMode::Shared)
        .unwrap();
    manager
        .acquire(first_tx, second_resource, LockMode::Shared)
        .unwrap();

    let decision =
        decide_deadlock_from_lock_manager(&manager, DeadlockPolicy::default(), None).unwrap();

    let DeadlockDecision::Deferred { reason, evidence } = decision else {
        panic!("expected deferred decision");
    };
    assert_eq!(reason, MISSING_TRANSACTION_ORDERING_METADATA_REASON);
    assert_eq!(evidence.wait_for_edge_count(), 2);
}

#[test]
fn decision_times_out_when_deadline_expired() {
    let manager = LockManager::new();
    let resource = LockResource::row(1, 2, 3).unwrap();
    let holder = TransactionId::new(1);
    let waiter = TransactionId::new(2);
    let policy = DeadlockPolicy::new(
        Duration::from_millis(5),
        DeadlockVictimPolicy::YoungestTransactionId,
    )
    .unwrap();
    let mut clock = ManualDeadlockClock::new();
    let deadline = DeadlockDetectionDeadline::from_policy(policy, &clock).unwrap();

    manager
        .acquire(holder, resource, LockMode::Exclusive)
        .unwrap();
    manager.acquire(waiter, resource, LockMode::Shared).unwrap();
    clock.advance(Duration::from_millis(5)).unwrap();

    let decision =
        decide_deadlock_from_lock_manager_with_deadline(&manager, policy, None, deadline, &clock)
            .unwrap();

    assert_eq!(
        decision,
        DeadlockDecision::TimedOut {
            reason: DEADLOCK_DETECTION_TIMEOUT_REASON,
            elapsed: Duration::from_millis(5),
            timeout: Duration::from_millis(5),
            evidence: DeadlockDecisionEvidence {
                lock_snapshot_resource_count: 1,
                wait_for_edges: vec![(waiter, holder)],
            },
        }
    );
}

#[test]
fn typed_decision_is_consumable_by_coordinator_without_direct_abort() {
    fn coordinator_pick_victim(decision: &DeadlockDecision) -> Option<TransactionId> {
        match decision {
            DeadlockDecision::VictimSuggested { victim, .. } => Some(victim.tx_id),
            DeadlockDecision::NoCycle { .. }
            | DeadlockDecision::Deferred { .. }
            | DeadlockDecision::TimedOut { .. } => None,
        }
    }

    let manager = LockManager::new();
    let first_resource = LockResource::row(1, 2, 3).unwrap();
    let second_resource = LockResource::row(1, 2, 4).unwrap();
    let first_tx = TransactionId::new(1);
    let second_tx = TransactionId::new(2);
    let metadata = metadata_table(&[(1, 1), (2, 2)]);
    let transaction_statuses = TransactionStatusTable::new();
    transaction_statuses
        .record(first_tx, TransactionStatus::InFlight)
        .unwrap();
    transaction_statuses
        .record(second_tx, TransactionStatus::InFlight)
        .unwrap();
    let first_status_before = transaction_statuses.status(first_tx);
    let second_status_before = transaction_statuses.status(second_tx);

    manager
        .acquire(first_tx, first_resource, LockMode::Exclusive)
        .unwrap();
    manager
        .acquire(second_tx, second_resource, LockMode::Exclusive)
        .unwrap();
    manager
        .acquire(second_tx, first_resource, LockMode::Shared)
        .unwrap();
    manager
        .acquire(first_tx, second_resource, LockMode::Shared)
        .unwrap();

    let decision =
        decide_deadlock_from_lock_manager(&manager, DeadlockPolicy::default(), Some(&metadata))
            .unwrap();

    assert_eq!(coordinator_pick_victim(&decision), Some(second_tx));
    assert_eq!(transaction_statuses.status(first_tx), first_status_before);
    assert_eq!(transaction_statuses.status(second_tx), second_status_before);
}

fn assert_cycle(
    status: DeadlockDetectionStatus,
    expected_participants: Vec<TransactionId>,
    expected_victim: TransactionId,
    expected_policy: DeadlockVictimPolicy,
) {
    let DeadlockDetectionStatus::CycleFound {
        victim,
        cycle_participants,
    } = status
    else {
        panic!("expected cycle found status");
    };

    assert_eq!(cycle_participants, expected_participants);
    assert_eq!(victim.tx_id, expected_victim);
    assert_eq!(victim.victim_policy, expected_policy);
    assert_eq!(victim.cycle_participants, expected_participants);
}

fn metadata_table(entries: &[(u64, u64)]) -> DeadlockTransactionMetadataTable {
    let mut metadata = DeadlockTransactionMetadataTable::new();
    for (tx_id, start_order) in entries {
        metadata
            .register_start_order(TransactionId::new(*tx_id), *start_order)
            .unwrap();
    }
    metadata
}
