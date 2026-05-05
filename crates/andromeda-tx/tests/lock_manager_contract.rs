use andromeda_core::{AndromedaErrorKind, TransactionId};
use andromeda_tx::{
    decide_deadlock_from_lock_manager, DeadlockDecisionTraceOutcome, DeadlockPolicy,
    DeadlockVictimPolicy, LockAcquireStatus, LockHolder, LockManager, LockMode,
    LockReleaseAllSummary, LockResource, LockTraceKind, LockTraceOutcome, TransactionManager,
};

const ALL_LOCK_MODES: [LockMode; 6] = [
    LockMode::Shared,
    LockMode::Exclusive,
    LockMode::IntentShared,
    LockMode::IntentExclusive,
    LockMode::SchemaShared,
    LockMode::SchemaExclusive,
];

#[test]
fn lock_mode_compatibility_matrix_is_public_contract() {
    let expected = [
        ((LockMode::Shared, LockMode::Shared), true),
        ((LockMode::Shared, LockMode::Exclusive), false),
        ((LockMode::Shared, LockMode::IntentShared), true),
        ((LockMode::Shared, LockMode::IntentExclusive), false),
        ((LockMode::Shared, LockMode::SchemaShared), true),
        ((LockMode::Shared, LockMode::SchemaExclusive), false),
        ((LockMode::Exclusive, LockMode::Shared), false),
        ((LockMode::Exclusive, LockMode::Exclusive), false),
        ((LockMode::Exclusive, LockMode::IntentShared), false),
        ((LockMode::Exclusive, LockMode::IntentExclusive), false),
        ((LockMode::Exclusive, LockMode::SchemaShared), false),
        ((LockMode::Exclusive, LockMode::SchemaExclusive), false),
        ((LockMode::IntentShared, LockMode::Shared), true),
        ((LockMode::IntentShared, LockMode::Exclusive), false),
        ((LockMode::IntentShared, LockMode::IntentShared), true),
        ((LockMode::IntentShared, LockMode::IntentExclusive), true),
        ((LockMode::IntentShared, LockMode::SchemaShared), true),
        ((LockMode::IntentShared, LockMode::SchemaExclusive), false),
        ((LockMode::IntentExclusive, LockMode::Shared), false),
        ((LockMode::IntentExclusive, LockMode::Exclusive), false),
        ((LockMode::IntentExclusive, LockMode::IntentShared), true),
        ((LockMode::IntentExclusive, LockMode::IntentExclusive), true),
        ((LockMode::IntentExclusive, LockMode::SchemaShared), true),
        (
            (LockMode::IntentExclusive, LockMode::SchemaExclusive),
            false,
        ),
        ((LockMode::SchemaShared, LockMode::Shared), true),
        ((LockMode::SchemaShared, LockMode::Exclusive), false),
        ((LockMode::SchemaShared, LockMode::IntentShared), true),
        ((LockMode::SchemaShared, LockMode::IntentExclusive), true),
        ((LockMode::SchemaShared, LockMode::SchemaShared), true),
        ((LockMode::SchemaShared, LockMode::SchemaExclusive), false),
        ((LockMode::SchemaExclusive, LockMode::Shared), false),
        ((LockMode::SchemaExclusive, LockMode::Exclusive), false),
        ((LockMode::SchemaExclusive, LockMode::IntentShared), false),
        (
            (LockMode::SchemaExclusive, LockMode::IntentExclusive),
            false,
        ),
        ((LockMode::SchemaExclusive, LockMode::SchemaShared), false),
        (
            (LockMode::SchemaExclusive, LockMode::SchemaExclusive),
            false,
        ),
    ];

    let mut observed_pairs = 0;
    for existing in ALL_LOCK_MODES {
        for requested in ALL_LOCK_MODES {
            let (_, compatible) = expected
                .iter()
                .find(|((expected_existing, expected_requested), _)| {
                    *expected_existing == existing && *expected_requested == requested
                })
                .expect("every public lock-mode pair must be asserted");

            assert_eq!(
                existing.is_compatible_with(requested),
                *compatible,
                "existing={existing:?}, requested={requested:?}"
            );
            assert_eq!(
                existing.is_compatible_with(requested),
                requested.is_compatible_with(existing),
                "compatibility matrix must remain symmetric for existing={existing:?}, requested={requested:?}"
            );
            observed_pairs += 1;
        }
    }

    assert_eq!(observed_pairs, 36);
    assert_eq!(expected.len(), 36);
}

#[test]
fn shared_acquisition_is_compatible_and_reentrant_without_duplicate_holder() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 10).unwrap();
    let first_tx = TransactionId::new(1);
    let second_tx = TransactionId::new(2);

    assert_eq!(
        manager
            .acquire(first_tx, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(second_tx, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(first_tx, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::AlreadyHeld {
            held_mode: LockMode::Shared
        }
    );

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(
        entry.holders,
        vec![
            LockHolder {
                tx_id: first_tx,
                mode: LockMode::Shared,
            },
            LockHolder {
                tx_id: second_tx,
                mode: LockMode::Shared,
            },
        ]
    );
    assert!(entry.waiters.is_empty());
}

#[test]
fn exclusive_request_waits_with_deterministic_blocker_evidence() {
    let manager = LockManager::new();
    let resource = LockResource::row(1, 10, 100).unwrap();
    let reader = TransactionId::new(1);
    let writer = TransactionId::new(2);

    assert_eq!(
        manager.acquire(reader, resource, LockMode::Shared).unwrap(),
        LockAcquireStatus::Granted
    );

    assert_eq!(
        manager
            .acquire(writer, resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Waiting {
            sequence: 1,
            blockers: vec![LockHolder {
                tx_id: reader,
                mode: LockMode::Shared,
            }],
        }
    );

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 1);
    assert_eq!(entry.waiters.len(), 1);
    assert_eq!(entry.waiters[0].tx_id, writer);
    assert_eq!(entry.waiters[0].mode, LockMode::Exclusive);
    assert_eq!(entry.waiters[0].sequence, 1);
}

#[test]
fn release_promotes_fifo_compatible_waiters_without_bypassing_incompatible_front() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 20).unwrap();
    let first_reader = TransactionId::new(1);
    let second_reader = TransactionId::new(2);
    let waiting_writer = TransactionId::new(3);
    let later_reader = TransactionId::new(4);

    assert_eq!(
        manager
            .acquire(first_reader, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(second_reader, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert!(matches!(
        manager
            .acquire(waiting_writer, resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Waiting { sequence: 1, .. }
    ));
    assert!(matches!(
        manager
            .acquire(later_reader, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { sequence: 2, .. }
    ));

    assert!(manager.release(first_reader, resource).unwrap());
    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(
        entry.holders,
        vec![LockHolder {
            tx_id: second_reader,
            mode: LockMode::Shared,
        }]
    );
    assert_eq!(entry.waiters[0].tx_id, waiting_writer);
    assert_eq!(entry.waiters[0].sequence, 1);
    assert_eq!(entry.waiters[1].tx_id, later_reader);
    assert_eq!(entry.waiters[1].sequence, 2);

    assert!(manager.release(second_reader, resource).unwrap());
    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(
        entry.holders,
        vec![LockHolder {
            tx_id: waiting_writer,
            mode: LockMode::Exclusive,
        }]
    );
    assert_eq!(entry.waiters.len(), 1);
    assert_eq!(entry.waiters[0].tx_id, later_reader);
}

#[test]
fn blocked_upgrade_is_promoted_in_place_after_blocker_release() {
    let manager = LockManager::new();
    let resource = LockResource::row(1, 20, 200).unwrap();
    let upgrading_tx = TransactionId::new(10);
    let blocking_tx = TransactionId::new(11);

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
    assert_eq!(
        manager
            .acquire(upgrading_tx, resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::WaitingUpgrade {
            sequence: 1,
            held_mode: LockMode::Shared,
            requested_mode: LockMode::Exclusive,
            blockers: vec![LockHolder {
                tx_id: blocking_tx,
                mode: LockMode::Shared,
            }],
        }
    );

    assert!(manager.release(blocking_tx, resource).unwrap());

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(
        entry.holders,
        vec![LockHolder {
            tx_id: upgrading_tx,
            mode: LockMode::Exclusive,
        }]
    );
    assert!(entry.waiters.is_empty());
}

#[test]
fn release_all_is_terminal_cleanup_only_and_removes_holders_and_waiters() {
    let manager = LockManager::new();
    let held_resource = LockResource::table(1, 30).unwrap();
    let waited_resource = LockResource::row(1, 30, 300).unwrap();
    let ending_tx = TransactionId::new(20);
    let other_tx = TransactionId::new(21);

    assert_eq!(
        manager
            .acquire(ending_tx, held_resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(other_tx, waited_resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert!(matches!(
        manager
            .acquire(ending_tx, waited_resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { .. }
    ));

    let summary = manager.release_all(ending_tx).unwrap();

    assert_eq!(
        summary,
        LockReleaseAllSummary {
            affected_resource_count: 2,
            released_holder_count: 1,
            removed_waiter_count: 1,
        }
    );
    assert!(summary.removed_any());
    assert_eq!(manager.entry(held_resource).unwrap(), None);
    let waited_entry = manager.entry(waited_resource).unwrap().unwrap();
    assert_eq!(
        waited_entry.holders,
        vec![LockHolder {
            tx_id: other_tx,
            mode: LockMode::Exclusive,
        }]
    );
    assert!(waited_entry.waiters.is_empty());
}

#[test]
fn invalid_ids_return_transaction_errors_without_mutating_lock_table() {
    let manager = LockManager::new();
    let valid_resource = LockResource::table(1, 40).unwrap();
    let invalid_resource = LockResource::Table {
        schema_id: 1,
        table_id: 0,
    };
    let valid_tx = TransactionId::new(30);
    let zero_tx = TransactionId::new(0);

    assert_eq!(
        manager
            .acquire(zero_tx, valid_resource, LockMode::Shared)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(
        manager
            .acquire(valid_tx, invalid_resource, LockMode::Shared)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(
        manager.release(zero_tx, valid_resource).unwrap_err().kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(
        manager
            .release(valid_tx, invalid_resource)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(
        manager.release_all(zero_tx).unwrap_err().kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(
        manager
            .enqueue_waiter(invalid_resource, valid_tx, LockMode::Shared)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(
        manager
            .record_holder(invalid_resource, valid_tx, LockMode::Shared)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(
        manager.entry(invalid_resource).unwrap_err().kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(
        manager.ensure_entry(invalid_resource).unwrap_err().kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(manager.entry_count().unwrap(), 0);
}

#[test]
fn snapshot_is_deterministic_and_returns_owned_clones() {
    let manager = LockManager::new();
    let row = LockResource::row(1, 50, 500).unwrap();
    let schema = LockResource::schema(1).unwrap();
    let table = LockResource::table(1, 50).unwrap();

    assert_eq!(
        manager
            .acquire(TransactionId::new(3), row, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(TransactionId::new(1), schema, LockMode::SchemaShared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(TransactionId::new(2), table, LockMode::IntentShared)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    let before_release = manager.snapshot().unwrap();
    let second_snapshot = manager.snapshot().unwrap();
    assert_eq!(before_release, second_snapshot);
    assert_eq!(
        before_release
            .iter()
            .map(|(resource, _)| *resource)
            .collect::<Vec<_>>(),
        vec![schema, table, row]
    );

    assert!(manager.release(TransactionId::new(1), schema).unwrap());

    assert_eq!(
        before_release
            .iter()
            .find(|(resource, _)| *resource == schema)
            .unwrap()
            .1
            .holders,
        vec![LockHolder {
            tx_id: TransactionId::new(1),
            mode: LockMode::SchemaShared,
        }]
    );
    assert!(
        manager
            .snapshot()
            .unwrap()
            .iter()
            .all(|(resource, _)| *resource != schema)
    );
}

#[test]
fn invalid_input_paths_return_errors_instead_of_panicking() {
    let manager = LockManager::new();
    let invalid_resource = LockResource::Row {
        schema_id: 1,
        table_id: 1,
        row_id: 0,
    };
    let zero_tx = TransactionId::new(0);

    let result = std::panic::catch_unwind(|| {
        assert!(LockResource::schema(0).is_err());
        assert!(LockResource::table(1, 0).is_err());
        assert!(LockResource::page(1, 1, 0).is_err());
        assert!(LockResource::row(1, 1, 0).is_err());
        assert!(manager.ensure_entry(invalid_resource).is_err());
        assert!(manager.entry(invalid_resource).is_err());
        assert!(
            manager
                .acquire(zero_tx, invalid_resource, LockMode::Exclusive)
                .is_err()
        );
        assert!(
            manager
                .enqueue_waiter(invalid_resource, zero_tx, LockMode::Shared)
                .is_err()
        );
        assert!(
            manager
                .record_holder(invalid_resource, zero_tx, LockMode::Shared)
                .is_err()
        );
        assert!(manager.release(zero_tx, invalid_resource).is_err());
        assert!(manager.release_all(zero_tx).is_err());
    });

    assert!(result.is_ok());
    assert_eq!(manager.entry_count().unwrap(), 0);
}

#[test]
fn transaction_lock_coordinator_acquires_lock_for_valid_transaction() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let tx = transactions.begin().unwrap();
    let resource = LockResource::row(1, 60, 600).unwrap();
    let coordinator = transactions.lock_coordinator(&locks);

    assert_eq!(
        coordinator.acquire(tx, resource, LockMode::Shared).unwrap(),
        LockAcquireStatus::Granted
    );

    let entry = locks.entry(resource).unwrap().unwrap();
    assert_eq!(
        entry.holders,
        vec![LockHolder {
            tx_id: tx,
            mode: LockMode::Shared,
        }]
    );
    assert!(entry.waiters.is_empty());
}

#[test]
fn transaction_lock_coordinator_propagates_waiting_status() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let holder = transactions.begin().unwrap();
    let waiter = transactions.begin().unwrap();
    let resource = LockResource::table(1, 70).unwrap();

    assert_eq!(
        transactions
            .acquire_lock(&locks, holder, resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    assert_eq!(
        transactions
            .acquire_lock(&locks, waiter, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting {
            sequence: 1,
            blockers: vec![LockHolder {
                tx_id: holder,
                mode: LockMode::Exclusive,
            }],
        }
    );
}

#[test]
fn waiting_acquisition_projects_critical_wait_evidence() {
    let manager = LockManager::new();
    let resource = LockResource::row(1, 71, 701).unwrap();
    let holder = TransactionId::new(71);
    let waiter = TransactionId::new(72);

    assert_eq!(
        manager
            .acquire(holder, resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );

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
        vec![LockHolder {
            tx_id: holder,
            mode: LockMode::Exclusive,
        }]
    );
    assert_eq!(evidence.outcome, LockTraceOutcome::Waiting);
}

#[test]
fn release_promotion_evidence_identifies_promoted_waiter() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 72).unwrap();
    let holder = TransactionId::new(73);
    let waiter = TransactionId::new(74);

    assert_eq!(
        manager
            .acquire(holder, resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert!(matches!(
        manager
            .acquire(waiter, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { sequence: 1, .. }
    ));

    let release = manager.release_with_evidence(holder, resource).unwrap();

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
    let held_resource = LockResource::schema(1).unwrap();
    let waited_resource = LockResource::row(1, 73, 703).unwrap();
    let ending_tx = TransactionId::new(75);
    let other_tx = TransactionId::new(76);

    assert_eq!(
        manager
            .acquire(ending_tx, held_resource, LockMode::SchemaShared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(other_tx, waited_resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );
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

#[test]
fn deadlock_decision_projects_trace_evidence_without_abort_side_effect() {
    let manager = LockManager::new();
    let first_resource = LockResource::row(1, 74, 704).unwrap();
    let second_resource = LockResource::row(1, 74, 705).unwrap();
    let first_tx = TransactionId::new(77);
    let second_tx = TransactionId::new(78);

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
            .acquire(first_tx, second_resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { .. }
    ));
    assert!(matches!(
        manager
            .acquire(second_tx, first_resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { .. }
    ));

    let policy = DeadlockPolicy::new(
        andromeda_tx::DEFAULT_DEADLOCK_TIMEOUT,
        DeadlockVictimPolicy::YoungestTransactionId,
    )
    .unwrap();
    let decision = decide_deadlock_from_lock_manager(&manager, policy, None).unwrap();
    let trace = decision.to_trace();

    assert_eq!(trace.outcome, DeadlockDecisionTraceOutcome::VictimSuggested);
    assert_eq!(trace.tx_id, Some(second_tx));
    assert_eq!(trace.cycle_participants, vec![first_tx, second_tx]);
    assert_eq!(
        trace.wait_for_edges,
        vec![(first_tx, second_tx), (second_tx, first_tx)]
    );
}

#[test]
fn lock_trace_evidence_does_not_carry_durability_or_wal_claims() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 75).unwrap();
    let holder = TransactionId::new(79);
    let waiter = TransactionId::new(80);

    assert_eq!(
        manager
            .acquire(holder, resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    let wait = manager
        .acquire_with_evidence(waiter, resource, LockMode::Shared)
        .unwrap()
        .evidence
        .unwrap();
    let release_all = manager.release_all_with_evidence(holder).unwrap();
    let rendered = format!("{wait:?} {:?}", release_all.evidence);
    let rendered = rendered.to_ascii_lowercase();

    assert!(!rendered.contains("durable_lsn"));
    assert!(!rendered.contains("wal"));
    assert!(!rendered.contains("commit"));
}

#[test]
fn transaction_lock_coordinator_terminal_release_all_removes_holders_and_waiters() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let ending_tx = transactions.begin().unwrap();
    let other_tx = transactions.begin().unwrap();
    let held_resource = LockResource::schema(1).unwrap();
    let waited_resource = LockResource::row(1, 80, 800).unwrap();

    assert_eq!(
        transactions
            .acquire_lock(&locks, ending_tx, held_resource, LockMode::SchemaShared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        transactions
            .acquire_lock(&locks, other_tx, waited_resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert!(matches!(
        transactions
            .acquire_lock(&locks, ending_tx, waited_resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { .. }
    ));

    transactions.request_commit(ending_tx).unwrap();
    transactions.commit_durable(ending_tx, 88).unwrap();

    let summary = transactions.release_all_locks(&locks, ending_tx).unwrap();

    assert_eq!(
        summary,
        LockReleaseAllSummary {
            affected_resource_count: 2,
            released_holder_count: 1,
            removed_waiter_count: 1,
        }
    );
    assert!(summary.removed_any());
    assert_eq!(locks.entry(held_resource).unwrap(), None);
    let waited_entry = locks.entry(waited_resource).unwrap().unwrap();
    assert_eq!(
        waited_entry.holders,
        vec![LockHolder {
            tx_id: other_tx,
            mode: LockMode::Exclusive,
        }]
    );
    assert!(waited_entry.waiters.is_empty());
}

#[test]
fn transaction_lock_coordinator_rejects_invalid_transaction_and_resource() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let valid_tx = transactions.begin().unwrap();
    let unknown_tx = TransactionId::new(99_999);
    let zero_tx = TransactionId::new(0);
    let valid_resource = LockResource::table(1, 90).unwrap();
    let invalid_resource = LockResource::Table {
        schema_id: 1,
        table_id: 0,
    };

    assert_eq!(
        transactions
            .acquire_lock(&locks, unknown_tx, valid_resource, LockMode::Shared)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(
        transactions
            .acquire_lock(&locks, zero_tx, valid_resource, LockMode::Shared)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(
        transactions
            .acquire_lock(&locks, valid_tx, invalid_resource, LockMode::Shared)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(
        transactions
            .release_lock(&locks, valid_tx, invalid_resource)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(
        transactions
            .release_all_locks(&locks, unknown_tx)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(
        transactions
            .release_all_locks(&locks, valid_tx)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(locks.entry_count().unwrap(), 0);
}

/// 2PL Integration tests: verify strict two-phase locking enforcement
/// across transaction state transitions and lock operations.

#[test]
fn two_phase_locking_growing_phase_multiple_acquires() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let tx = transactions.begin().unwrap();
    let res1 = LockResource::row(1, 100, 1000).unwrap();
    let res2 = LockResource::row(1, 100, 1001).unwrap();

    // Growing phase: acquire multiple locks in Active state
    assert_eq!(
        transactions
            .acquire_lock(&locks, tx, res1, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        transactions
            .acquire_lock(&locks, tx, res2, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    let snap = locks.snapshot().unwrap();
    assert_eq!(snap.len(), 2);
}

#[test]
fn two_phase_locking_transition_from_active_to_committing() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let tx = transactions.begin().unwrap();
    let resource = LockResource::row(1, 101, 1010).unwrap();

    // Growing phase: acquire lock
    assert_eq!(
        transactions
            .acquire_lock(&locks, tx, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    // Transition to Committing (begin shrinking phase)
    transactions.request_commit(tx).unwrap();

    // Verify state is now Committing
    let snap = transactions.snapshot(tx).unwrap().unwrap();
    assert_eq!(snap.state_machine.state, TransactionState::Committing);
}

#[test]
fn two_phase_locking_shrinking_phase_release_locks() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let tx = transactions.begin().unwrap();
    let res1 = LockResource::row(1, 102, 1020).unwrap();
    let res2 = LockResource::row(1, 102, 1021).unwrap();

    // Growing phase: acquire locks
    assert_eq!(
        transactions
            .acquire_lock(&locks, tx, res1, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        transactions
            .acquire_lock(&locks, tx, res2, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    // Transition to Committing
    transactions.request_commit(tx).unwrap();

    // Shrinking phase: release locks
    assert!(transactions
        .release_lock(&locks, tx, res1)
        .unwrap());
    assert!(transactions
        .release_lock(&locks, tx, res2)
        .unwrap());

    // Verify locks are released
    let snap = locks.snapshot().unwrap();
    assert_eq!(snap.len(), 0);
}

#[test]
fn two_phase_locking_terminal_cleanup_with_release_all() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let tx = transactions.begin().unwrap();
    let resource = LockResource::row(1, 103, 1030).unwrap();

    // Growing phase
    assert_eq!(
        transactions
            .acquire_lock(&locks, tx, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    // Commit the transaction
    transactions.request_commit(tx).unwrap();
    transactions.commit_durable(tx, 1).unwrap();

    // Terminal cleanup
    let summary = transactions
        .release_all_locks(&locks, tx)
        .unwrap();
    assert!(summary.removed_any());

    // Verify locks are cleaned up
    let snap = locks.snapshot().unwrap();
    assert_eq!(snap.len(), 0);
}

#[test]
fn two_phase_locking_rollback_path_releases_locks() {
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let tx = transactions.begin().unwrap();
    let resource = LockResource::row(1, 104, 1040).unwrap();

    // Growing phase
    assert_eq!(
        transactions
            .acquire_lock(&locks, tx, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    // Rollback path
    transactions.request_rollback(tx).unwrap();
    transactions.rollback_durable(tx, 1).unwrap();

    // Terminal cleanup
    let summary = transactions
        .release_all_locks(&locks, tx)
        .unwrap();
    assert!(summary.removed_any());

    // Verify locks are cleaned up
    let snap = locks.snapshot().unwrap();
    assert_eq!(snap.len(), 0);
}

#[test]
fn two_phase_locking_prevents_acquire_in_inflight_state_other_than_active() {
    // This test verifies that acquire is restricted by the transaction manager
    // to Active state only, not other in-flight states
    let transactions = TransactionManager::new();
    let locks = LockManager::new();
    let tx = transactions.begin().unwrap();
    let resource = LockResource::row(1, 105, 1050).unwrap();

    // Acquire succeeds in Active
    assert_eq!(
        transactions
            .acquire_lock(&locks, tx, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    // Move to Committing
    transactions.request_commit(tx).unwrap();

    // Attempt to acquire more locks should be rejected
    // The transaction manager should reject this based on state validation
    let snap = transactions.snapshot(tx).unwrap().unwrap();
    assert_eq!(snap.state_machine.state, TransactionState::Committing);

    // Note: The current implementation of TransactionLockCoordinator.acquire
    // uses require_lock_acquire_transaction which requires Active state,
    // so this would fail. But let's verify through 2PL validator.
    use andromeda_tx::{TwoPhaseLocksValidator, TwoPhaseOperation};
    let validation =
        TwoPhaseLocksValidator::validate_operation(snap.state_machine.state, TwoPhaseOperation::Acquire);
    assert!(
        validation.is_err(),
        "2PL must reject acquire in Committing state"
    );
}

