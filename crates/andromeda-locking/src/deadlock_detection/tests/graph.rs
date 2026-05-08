use super::*;

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
