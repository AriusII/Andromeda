use super::*;

pub(in crate::deadlock_detection::tests) struct TwoResourceDeadlockFixture {
    pub(in crate::deadlock_detection::tests) manager: LockManager,
    pub(in crate::deadlock_detection::tests) first_tx: TransactionId,
    pub(in crate::deadlock_detection::tests) second_tx: TransactionId,
    pub(in crate::deadlock_detection::tests) metadata: DeadlockTransactionMetadataTable,
}

pub(in crate::deadlock_detection::tests) fn assert_cycle(
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

pub(in crate::deadlock_detection::tests) fn detector_with_policy(
    victim_policy: DeadlockVictimPolicy,
) -> DeadlockDetector {
    DeadlockDetector::new(DeadlockPolicy::new(DEFAULT_DEADLOCK_TIMEOUT, victim_policy).unwrap())
        .unwrap()
}

pub(in crate::deadlock_detection::tests) fn graph_with_edges(edges: &[(u64, u64)]) -> WaitForGraph {
    let mut graph = WaitForGraph::new();
    for (waiter, blocker) in edges {
        graph.insert_edge(tx(*waiter), tx(*blocker)).unwrap();
    }
    graph
}

pub(in crate::deadlock_detection::tests) fn two_resource_deadlock_fixture(
    metadata_entries: &[(u64, u64)],
) -> TwoResourceDeadlockFixture {
    let manager = LockManager::new();
    let first_resource = LockResource::row(1, 2, 3).unwrap();
    let second_resource = LockResource::row(1, 2, 4).unwrap();
    let first_tx = tx(1);
    let second_tx = tx(2);

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

    TwoResourceDeadlockFixture {
        manager,
        first_tx,
        second_tx,
        metadata: metadata_table(metadata_entries),
    }
}

pub(in crate::deadlock_detection::tests) fn metadata_table(
    entries: &[(u64, u64)],
) -> DeadlockTransactionMetadataTable {
    let mut metadata = DeadlockTransactionMetadataTable::new();
    for (tx_id, start_order) in entries {
        metadata
            .register_start_order(tx(*tx_id), *start_order)
            .unwrap();
    }
    metadata
}

pub(in crate::deadlock_detection::tests) fn tx(id: u64) -> TransactionId {
    TransactionId::new(id)
}
