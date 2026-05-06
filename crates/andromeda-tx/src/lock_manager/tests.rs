use super::*;
use andromeda_core::{AndromedaErrorKind, TransactionId};

const ALL_LOCK_MODES: [LockMode; 6] = [
    LockMode::Shared,
    LockMode::Exclusive,
    LockMode::IntentShared,
    LockMode::IntentExclusive,
    LockMode::SchemaShared,
    LockMode::SchemaExclusive,
];

#[test]
fn resource_constructors_reject_zero_components() {
    assert!(LockResource::schema(0).is_err());
    assert!(LockResource::object(0, 1).is_err());
    assert!(LockResource::object(1, 0).is_err());
    assert!(LockResource::table(1, 0).is_err());
    assert!(LockResource::page(1, 1, 0).is_err());
    assert!(LockResource::row(1, 1, 0).is_err());
}

#[test]
fn lock_types_construct_with_valid_inputs() {
    let tx_id = TransactionId::new(1);
    let holder = LockHolder::new(tx_id, LockMode::Shared).unwrap();
    let waiter = LockWaiter::new(tx_id, LockMode::Exclusive, 7).unwrap();

    assert_eq!(holder.tx_id, tx_id);
    assert_eq!(holder.mode, LockMode::Shared);
    assert_eq!(waiter.sequence, 7);
}

#[test]
fn lock_mode_v0_compatibility_matrix_is_exhaustive() {
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
                .expect("every lock-mode pair must be listed");

            assert_eq!(
                existing.is_compatible_with(requested),
                *compatible,
                "existing={existing:?}, requested={requested:?}"
            );
            observed_pairs += 1;
        }
    }

    assert_eq!(observed_pairs, 36);
    assert_eq!(expected.len(), 36);
}

#[test]
fn lock_mode_v0_compatibility_matrix_is_symmetric() {
    for existing in ALL_LOCK_MODES {
        for requested in ALL_LOCK_MODES {
            assert_eq!(
                existing.is_compatible_with(requested),
                requested.is_compatible_with(existing),
                "existing={existing:?}, requested={requested:?}"
            );
        }
    }
}

#[test]
fn catalog_intention_plans_are_typed_and_catalog_agnostic() {
    let invocation = CatalogIntentionLocks::procedure_invocation(10, 20).unwrap();
    assert_eq!(
        invocation,
        [
            LockRequest::new(
                LockResource::Schema { schema_id: 10 },
                LockMode::IntentShared
            ),
            LockRequest::new(
                LockResource::Object {
                    schema_id: 10,
                    object_id: 20,
                },
                LockMode::SchemaShared
            ),
        ]
    );

    let mutation = CatalogIntentionLocks::definition_batch_object_mutation(10, 20).unwrap();
    assert_eq!(
        mutation,
        [
            LockRequest::new(
                LockResource::Schema { schema_id: 10 },
                LockMode::IntentExclusive
            ),
            LockRequest::new(
                LockResource::Object {
                    schema_id: 10,
                    object_id: 20,
                },
                LockMode::SchemaExclusive
            ),
        ]
    );

    let schema_mutation = CatalogIntentionLocks::definition_batch_schema_mutation(10).unwrap();
    assert_eq!(
        schema_mutation,
        [LockRequest::new(
            LockResource::Schema { schema_id: 10 },
            LockMode::SchemaExclusive
        )]
    );
}

#[test]
fn catalog_intention_plans_reject_zero_identifiers() {
    assert!(CatalogIntentionLocks::procedure_invocation(0, 20).is_err());
    assert!(CatalogIntentionLocks::procedure_invocation(10, 0).is_err());
    assert!(CatalogIntentionLocks::definition_batch_object_mutation(0, 20).is_err());
    assert!(CatalogIntentionLocks::definition_batch_object_mutation(10, 0).is_err());
    assert!(CatalogIntentionLocks::definition_batch_schema_mutation(0).is_err());
}

#[test]
fn catalog_intention_matrix_blocks_definition_batch_against_invocation() {
    let manager = LockManager::new();
    let invocation_tx = TransactionId::new(1);
    let ddl_tx = TransactionId::new(2);

    for request in CatalogIntentionLocks::procedure_invocation(10, 20).unwrap() {
        assert_eq!(
            manager
                .acquire(invocation_tx, request.resource, request.mode)
                .unwrap(),
            LockAcquireStatus::Granted
        );
    }

    let mut statuses = Vec::new();
    for request in CatalogIntentionLocks::definition_batch_object_mutation(10, 20).unwrap() {
        statuses.push(
            manager
                .acquire(ddl_tx, request.resource, request.mode)
                .unwrap(),
        );
    }

    assert_eq!(statuses[0], LockAcquireStatus::Granted);
    assert!(matches!(
        &statuses[1],
        LockAcquireStatus::Waiting { blockers, .. }
            if blockers == &vec![LockHolder {
                tx_id: invocation_tx,
                mode: LockMode::SchemaShared,
            }]
    ));
}

#[test]
fn schema_level_definition_batch_blocks_invocation_ancestor_intent() {
    let manager = LockManager::new();
    let ddl_tx = TransactionId::new(1);
    let invocation_tx = TransactionId::new(2);

    for request in CatalogIntentionLocks::definition_batch_schema_mutation(10).unwrap() {
        assert_eq!(
            manager
                .acquire(ddl_tx, request.resource, request.mode)
                .unwrap(),
            LockAcquireStatus::Granted
        );
    }

    let invocation = CatalogIntentionLocks::procedure_invocation(10, 20).unwrap();
    let status = manager
        .acquire(invocation_tx, invocation[0].resource, invocation[0].mode)
        .unwrap();
    assert!(matches!(
        status,
        LockAcquireStatus::Waiting { blockers, .. }
            if blockers == vec![LockHolder {
                tx_id: ddl_tx,
                mode: LockMode::SchemaExclusive,
            }]
    ));
}

#[test]
fn manager_tracks_entries_and_waiters() {
    let manager = LockManager::new();
    let resource = LockResource::row(1, 2, 3).unwrap();
    let tx_id = TransactionId::new(11);

    manager.ensure_entry(resource).unwrap();
    assert_eq!(manager.entry_count().unwrap(), 1);

    let sequence = manager
        .enqueue_waiter(resource, tx_id, LockMode::Exclusive)
        .unwrap();
    assert_eq!(sequence, 1);

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 0);
    assert_eq!(entry.waiters.len(), 1);
    assert_eq!(entry.waiters[0].tx_id, tx_id);
}

#[test]
fn acquire_grants_immediately_when_compatible() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();

    let first = manager
        .acquire(TransactionId::new(1), resource, LockMode::Shared)
        .unwrap();
    let second = manager
        .acquire(TransactionId::new(2), resource, LockMode::Shared)
        .unwrap();

    assert_eq!(first, LockAcquireStatus::Granted);
    assert_eq!(second, LockAcquireStatus::Granted);

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(
        entry.holders,
        vec![
            LockHolder {
                tx_id: TransactionId::new(1),
                mode: LockMode::Shared,
            },
            LockHolder {
                tx_id: TransactionId::new(2),
                mode: LockMode::Shared,
            },
        ]
    );
    assert!(entry.waiters.is_empty());
}

#[test]
fn acquire_waits_with_blockers_when_incompatible() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();

    assert_eq!(
        manager
            .acquire(TransactionId::new(1), resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    let status = manager
        .acquire(TransactionId::new(2), resource, LockMode::Shared)
        .unwrap();

    assert_eq!(
        status,
        LockAcquireStatus::Waiting {
            sequence: 1,
            blockers: vec![LockHolder {
                tx_id: TransactionId::new(1),
                mode: LockMode::Exclusive,
            }],
        }
    );

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 1);
    assert_eq!(entry.waiters.len(), 1);
    assert_eq!(entry.waiters[0].tx_id, TransactionId::new(2));
    assert_eq!(entry.waiters[0].sequence, 1);
}

#[test]
fn acquire_reports_same_transaction_already_held_without_duplicate_holder() {
    let manager = LockManager::new();
    let resource = LockResource::row(1, 2, 3).unwrap();
    let tx_id = TransactionId::new(7);

    assert_eq!(
        manager.acquire(tx_id, resource, LockMode::Shared).unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager.acquire(tx_id, resource, LockMode::Shared).unwrap(),
        LockAcquireStatus::AlreadyHeld {
            held_mode: LockMode::Shared,
        }
    );

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 1);
    assert!(entry.waiters.is_empty());
}

#[test]
fn acquire_upgrades_shared_to_exclusive_in_place_when_unblocked() {
    let manager = LockManager::new();
    let resource = LockResource::row(1, 2, 3).unwrap();
    let tx_id = TransactionId::new(7);

    assert_eq!(
        manager.acquire(tx_id, resource, LockMode::Shared).unwrap(),
        LockAcquireStatus::Granted
    );

    assert_eq!(
        manager
            .acquire(tx_id, resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Upgraded {
            previous_mode: LockMode::Shared,
            new_mode: LockMode::Exclusive,
        }
    );

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 1);
    assert_eq!(entry.holders[0].tx_id, tx_id);
    assert_eq!(entry.holders[0].mode, LockMode::Exclusive);
    assert!(entry.waiters.is_empty());
}

#[test]
fn acquire_waits_upgrade_when_other_holders_block_shared_to_exclusive() {
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

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 2);
    assert_eq!(entry.holders[0].mode, LockMode::Shared);
    assert_eq!(entry.waiters.len(), 1);
    assert_eq!(entry.waiters[0].tx_id, upgrading_tx);
    assert_eq!(entry.waiters[0].mode, LockMode::Exclusive);
    assert_eq!(entry.waiters[0].sequence, 1);
}

#[test]
fn acquire_upgrades_intent_shared_to_intent_exclusive_when_compatible() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();
    let upgrading_tx = TransactionId::new(10);
    let compatible_tx = TransactionId::new(11);

    assert_eq!(
        manager
            .acquire(upgrading_tx, resource, LockMode::IntentShared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(compatible_tx, resource, LockMode::IntentShared)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    assert_eq!(
        manager
            .acquire(upgrading_tx, resource, LockMode::IntentExclusive)
            .unwrap(),
        LockAcquireStatus::Upgraded {
            previous_mode: LockMode::IntentShared,
            new_mode: LockMode::IntentExclusive,
        }
    );

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 2);
    assert_eq!(
        entry
            .holders
            .iter()
            .find(|holder| holder.tx_id == upgrading_tx)
            .unwrap()
            .mode,
        LockMode::IntentExclusive
    );
    assert!(entry.waiters.is_empty());
}

#[test]
fn acquire_does_not_bypass_older_incompatible_waiter() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();

    assert_eq!(
        manager
            .acquire(TransactionId::new(1), resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(TransactionId::new(2), resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Waiting {
            sequence: 1,
            blockers: vec![LockHolder {
                tx_id: TransactionId::new(1),
                mode: LockMode::Shared,
            }],
        }
    );

    assert_eq!(
        manager
            .acquire(TransactionId::new(3), resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting {
            sequence: 2,
            blockers: vec![],
        }
    );

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 1);
    assert_eq!(entry.waiters.len(), 2);
    assert_eq!(entry.waiters[0].tx_id, TransactionId::new(2));
    assert_eq!(entry.waiters[0].sequence, 1);
    assert_eq!(entry.waiters[1].tx_id, TransactionId::new(3));
    assert_eq!(entry.waiters[1].sequence, 2);
}

#[test]
fn acquire_reentrant_waiting_upgrade_does_not_duplicate_waiter_or_holder() {
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

    let first = manager
        .acquire(upgrading_tx, resource, LockMode::Exclusive)
        .unwrap();
    let second = manager
        .acquire(upgrading_tx, resource, LockMode::Exclusive)
        .unwrap();

    assert!(matches!(
        first,
        LockAcquireStatus::WaitingUpgrade { sequence: 1, .. }
    ));
    assert!(matches!(
        second,
        LockAcquireStatus::WaitingUpgrade { sequence: 1, .. }
    ));

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 2);
    assert_eq!(
        entry
            .holders
            .iter()
            .filter(|holder| holder.tx_id == upgrading_tx)
            .count(),
        1
    );
    assert_eq!(entry.waiters.len(), 1);
    assert_eq!(entry.waiters[0].tx_id, upgrading_tx);
    assert_eq!(entry.waiters[0].sequence, 1);
}

#[test]
fn acquire_stronger_held_mode_reports_already_held_without_duplicate() {
    let manager = LockManager::new();
    let resource = LockResource::row(1, 2, 3).unwrap();
    let tx_id = TransactionId::new(7);

    assert_eq!(
        manager
            .acquire(tx_id, resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager.acquire(tx_id, resource, LockMode::Shared).unwrap(),
        LockAcquireStatus::AlreadyHeld {
            held_mode: LockMode::Exclusive,
        }
    );

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 1);
    assert_eq!(entry.holders[0].mode, LockMode::Exclusive);
    assert!(entry.waiters.is_empty());
}

#[test]
fn acquire_waiter_sequences_are_fifo_and_deterministic() {
    let manager = LockManager::new();
    let resource = LockResource::schema(1).unwrap();

    assert_eq!(
        manager
            .acquire(TransactionId::new(1), resource, LockMode::SchemaExclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    let first_wait = manager
        .acquire(TransactionId::new(2), resource, LockMode::SchemaShared)
        .unwrap();
    let second_wait = manager
        .acquire(TransactionId::new(3), resource, LockMode::Shared)
        .unwrap();

    assert!(matches!(
        first_wait,
        LockAcquireStatus::Waiting { sequence: 1, .. }
    ));
    assert!(matches!(
        second_wait,
        LockAcquireStatus::Waiting { sequence: 2, .. }
    ));

    let entry = manager.entry(resource).unwrap().unwrap();
    let waiters: Vec<(TransactionId, u64)> = entry
        .waiters
        .iter()
        .map(|waiter| (waiter.tx_id, waiter.sequence))
        .collect();
    assert_eq!(
        waiters,
        vec![(TransactionId::new(2), 1), (TransactionId::new(3), 2)]
    );
}

#[test]
fn release_removes_holder_and_keeps_non_empty_entry() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();

    assert_eq!(
        manager
            .acquire(TransactionId::new(1), resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(TransactionId::new(2), resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    assert!(manager.release(TransactionId::new(1), resource).unwrap());

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(
        entry.holders,
        vec![LockHolder {
            tx_id: TransactionId::new(2),
            mode: LockMode::Shared,
        }]
    );
    assert!(entry.waiters.is_empty());
    assert_eq!(manager.entry_count().unwrap(), 1);
}

#[test]
fn release_promotes_compatible_shared_waiters_in_fifo_order() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();

    assert_eq!(
        manager
            .acquire(TransactionId::new(1), resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert!(matches!(
        manager
            .acquire(TransactionId::new(2), resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { sequence: 1, .. }
    ));
    assert!(matches!(
        manager
            .acquire(TransactionId::new(3), resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { sequence: 2, .. }
    ));

    assert!(manager.release(TransactionId::new(1), resource).unwrap());

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(
        entry.holders,
        vec![
            LockHolder {
                tx_id: TransactionId::new(2),
                mode: LockMode::Shared,
            },
            LockHolder {
                tx_id: TransactionId::new(3),
                mode: LockMode::Shared,
            },
        ]
    );
    assert!(entry.waiters.is_empty());
}

#[test]
fn release_promotes_waiting_exclusive_when_compatible_after_holder_removal() {
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

    assert!(manager.release(TransactionId::new(1), resource).unwrap());

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(
        entry.holders,
        vec![LockHolder {
            tx_id: TransactionId::new(2),
            mode: LockMode::Exclusive,
        }]
    );
    assert!(entry.waiters.is_empty());
}

#[test]
fn release_preserves_fifo_when_incompatible_waiter_blocks_later_compatible_waiter() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();

    assert_eq!(
        manager
            .acquire(TransactionId::new(1), resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(TransactionId::new(2), resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert!(matches!(
        manager
            .acquire(TransactionId::new(3), resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Waiting { sequence: 1, .. }
    ));
    assert!(matches!(
        manager
            .acquire(TransactionId::new(4), resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { sequence: 2, .. }
    ));

    assert!(manager.release(TransactionId::new(1), resource).unwrap());

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(
        entry.holders,
        vec![LockHolder {
            tx_id: TransactionId::new(2),
            mode: LockMode::Shared,
        }]
    );
    let waiters: Vec<(TransactionId, LockMode, u64)> = entry
        .waiters
        .iter()
        .map(|waiter| (waiter.tx_id, waiter.mode, waiter.sequence))
        .collect();
    assert_eq!(
        waiters,
        vec![
            (TransactionId::new(3), LockMode::Exclusive, 1),
            (TransactionId::new(4), LockMode::Shared, 2),
        ]
    );
}

#[test]
fn release_promotes_waiting_upgrade_in_place() {
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

    assert!(manager.release(blocking_tx, resource).unwrap());

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 1);
    assert_eq!(entry.holders[0].tx_id, upgrading_tx);
    assert_eq!(entry.holders[0].mode, LockMode::Exclusive);
    assert!(entry.waiters.is_empty());
}

#[test]
fn release_removes_empty_entry_after_final_holder_release() {
    let manager = LockManager::new();
    let resource = LockResource::schema(1).unwrap();

    assert_eq!(
        manager
            .acquire(TransactionId::new(1), resource, LockMode::SchemaShared)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    assert!(manager.release(TransactionId::new(1), resource).unwrap());
    assert_eq!(manager.entry(resource).unwrap(), None);
    assert_eq!(manager.entry_count().unwrap(), 0);
}

#[test]
fn release_promotion_does_not_create_duplicate_holder_for_waiting_upgrade() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();
    let upgrading_tx = TransactionId::new(10);
    let blocking_tx = TransactionId::new(11);

    assert_eq!(
        manager
            .acquire(upgrading_tx, resource, LockMode::IntentShared)
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
            .acquire(upgrading_tx, resource, LockMode::IntentExclusive)
            .unwrap(),
        LockAcquireStatus::WaitingUpgrade { sequence: 1, .. }
    ));

    assert!(manager.release(blocking_tx, resource).unwrap());

    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(
        entry
            .holders
            .iter()
            .filter(|holder| holder.tx_id == upgrading_tx)
            .count(),
        1
    );
    assert_eq!(entry.holders[0].mode, LockMode::IntentExclusive);
    assert!(entry.waiters.is_empty());
}

#[test]
fn release_all_releases_multiple_resources_and_keeps_unaffected_holders() {
    let manager = LockManager::new();
    let table = LockResource::table(1, 2).unwrap();
    let row = LockResource::row(1, 2, 3).unwrap();
    let ending_tx = TransactionId::new(21);
    let remaining_tx = TransactionId::new(22);

    assert_eq!(
        manager.acquire(ending_tx, table, LockMode::Shared).unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager.acquire(ending_tx, row, LockMode::Shared).unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(remaining_tx, table, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    let summary = manager.release_all(ending_tx).unwrap();

    assert_eq!(
        summary,
        LockReleaseAllSummary {
            affected_resource_count: 2,
            released_holder_count: 2,
            removed_waiter_count: 0,
        }
    );
    assert!(summary.removed_any());
    let table_entry = manager.entry(table).unwrap().unwrap();
    assert_eq!(
        table_entry.holders,
        vec![LockHolder {
            tx_id: remaining_tx,
            mode: LockMode::Shared,
        }]
    );
    assert!(table_entry.waiters.is_empty());
    assert_eq!(manager.entry(row).unwrap(), None);
    assert_eq!(manager.entry_count().unwrap(), 1);
}

#[test]
fn release_all_removes_waiters_for_ending_transaction() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();
    let holder_tx = TransactionId::new(31);
    let waiting_tx = TransactionId::new(32);

    assert_eq!(
        manager
            .acquire(holder_tx, resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert!(matches!(
        manager
            .acquire(waiting_tx, resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { .. }
    ));

    let summary = manager.release_all(waiting_tx).unwrap();

    assert_eq!(
        summary,
        LockReleaseAllSummary {
            affected_resource_count: 1,
            released_holder_count: 0,
            removed_waiter_count: 1,
        }
    );
    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(
        entry.holders,
        vec![LockHolder {
            tx_id: holder_tx,
            mode: LockMode::Exclusive,
        }]
    );
    assert!(entry.waiters.is_empty());
}

#[test]
fn release_all_promotes_waiters_per_resource_after_holder_removal() {
    let manager = LockManager::new();
    let table = LockResource::table(1, 2).unwrap();
    let row = LockResource::row(1, 2, 3).unwrap();
    let ending_tx = TransactionId::new(41);
    let table_waiter = TransactionId::new(42);
    let row_waiter = TransactionId::new(43);

    assert_eq!(
        manager
            .acquire(ending_tx, table, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(ending_tx, row, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert!(matches!(
        manager
            .acquire(table_waiter, table, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Waiting { .. }
    ));
    assert!(matches!(
        manager.acquire(row_waiter, row, LockMode::Shared).unwrap(),
        LockAcquireStatus::Waiting { .. }
    ));

    let summary = manager.release_all(ending_tx).unwrap();

    assert_eq!(
        summary,
        LockReleaseAllSummary {
            affected_resource_count: 2,
            released_holder_count: 2,
            removed_waiter_count: 0,
        }
    );
    let table_entry = manager.entry(table).unwrap().unwrap();
    assert_eq!(
        table_entry.holders,
        vec![LockHolder {
            tx_id: table_waiter,
            mode: LockMode::Shared,
        }]
    );
    assert!(table_entry.waiters.is_empty());

    let row_entry = manager.entry(row).unwrap().unwrap();
    assert_eq!(
        row_entry.holders,
        vec![LockHolder {
            tx_id: row_waiter,
            mode: LockMode::Shared,
        }]
    );
    assert!(row_entry.waiters.is_empty());
}

#[test]
fn release_all_removes_empty_entries_after_terminal_cleanup() {
    let manager = LockManager::new();
    let schema = LockResource::schema(1).unwrap();
    let row = LockResource::row(1, 2, 3).unwrap();
    let ending_tx = TransactionId::new(51);

    assert_eq!(
        manager
            .acquire(ending_tx, schema, LockMode::SchemaShared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    manager
        .enqueue_waiter(row, ending_tx, LockMode::Exclusive)
        .unwrap();

    let summary = manager.release_all(ending_tx).unwrap();

    assert_eq!(
        summary,
        LockReleaseAllSummary {
            affected_resource_count: 2,
            released_holder_count: 1,
            removed_waiter_count: 1,
        }
    );
    assert_eq!(manager.entry(schema).unwrap(), None);
    assert_eq!(manager.entry(row).unwrap(), None);
    assert_eq!(manager.entry_count().unwrap(), 0);
}

#[test]
fn release_all_rejects_zero_transaction_id_without_mutation() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();

    assert_eq!(
        manager
            .acquire(TransactionId::new(61), resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    let err = manager.release_all(TransactionId::new(0)).unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(manager.entry_count().unwrap(), 1);
    let entry = manager.entry(resource).unwrap().unwrap();
    assert_eq!(entry.holders.len(), 1);
    assert!(entry.waiters.is_empty());
}

#[test]
fn release_all_leaves_unaffected_resource_entries_unchanged() {
    let manager = LockManager::new();
    let ending_resource = LockResource::table(1, 2).unwrap();
    let unaffected_resource = LockResource::row(1, 2, 3).unwrap();
    let ending_tx = TransactionId::new(71);
    let unaffected_tx = TransactionId::new(72);

    assert_eq!(
        manager
            .acquire(ending_tx, ending_resource, LockMode::Shared)
            .unwrap(),
        LockAcquireStatus::Granted
    );
    assert_eq!(
        manager
            .acquire(unaffected_tx, unaffected_resource, LockMode::Exclusive)
            .unwrap(),
        LockAcquireStatus::Granted
    );

    let summary = manager.release_all(ending_tx).unwrap();

    assert_eq!(
        summary,
        LockReleaseAllSummary {
            affected_resource_count: 1,
            released_holder_count: 1,
            removed_waiter_count: 0,
        }
    );
    assert_eq!(manager.entry(ending_resource).unwrap(), None);
    let unaffected_entry = manager.entry(unaffected_resource).unwrap().unwrap();
    assert_eq!(
        unaffected_entry.holders,
        vec![LockHolder {
            tx_id: unaffected_tx,
            mode: LockMode::Exclusive,
        }]
    );
    assert!(unaffected_entry.waiters.is_empty());
    assert_eq!(manager.entry_count().unwrap(), 1);
}

#[test]
fn acquire_rejects_invalid_transaction_id() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();

    let zero_tx = manager.acquire(TransactionId::new(0), resource, LockMode::Shared);
    assert_eq!(zero_tx.unwrap_err().kind(), AndromedaErrorKind::Transaction);
    assert_eq!(manager.entry_count().unwrap(), 0);
}

#[test]
fn acquire_rejects_invalid_resource_id() {
    let manager = LockManager::new();
    let resource = LockResource::Table {
        schema_id: 1,
        table_id: 0,
    };

    let invalid_resource = manager.acquire(TransactionId::new(1), resource, LockMode::Shared);
    assert_eq!(
        invalid_resource.unwrap_err().kind(),
        AndromedaErrorKind::Transaction
    );
    assert_eq!(manager.entry_count().unwrap(), 0);
}

#[test]
fn acquire_does_not_panic_on_grant_wait_or_reentrant_paths() {
    let manager = LockManager::new();
    let resource = LockResource::table(1, 2).unwrap();

    let result = std::panic::catch_unwind(|| {
        manager
            .acquire(TransactionId::new(1), resource, LockMode::Shared)
            .unwrap();
        manager
            .acquire(TransactionId::new(1), resource, LockMode::Shared)
            .unwrap();
        manager
            .acquire(TransactionId::new(2), resource, LockMode::Exclusive)
            .unwrap();
    });

    assert!(result.is_ok());
}
