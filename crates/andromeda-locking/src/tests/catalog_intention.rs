use super::*;

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
