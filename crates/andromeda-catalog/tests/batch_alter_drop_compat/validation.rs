use super::common::*;

/// **Verification: Batch rejects duplicate object IDs in single batch**
///
/// This validates the infrastructure that will prevent Alter/Drop from
/// being applied to the same object twice in one batch.
#[test]
fn test_batch_rejects_duplicate_object_ids() {
    let base_version = CatalogVersion::new(1);
    let next_version = CatalogVersion::new(2);

    let proc1 = procedure_contract(300, "test.Dup1", next_version);
    let proc2 = procedure_contract(300, "test.Dup2", next_version);

    let batch = definition_batch(
        base_version,
        vec![
            DefinitionOperation::Create(CatalogDefinition::Procedure(proc1)),
            DefinitionOperation::Create(CatalogDefinition::Procedure(proc2)),
        ],
    );

    let result = batch.dry_run();
    assert!(result.is_err(), "batch must reject duplicate object IDs");
    assert!(
        result
            .unwrap_err()
            .message()
            .contains("same lifecycle object id twice"),
        "error must indicate duplicate ID issue"
    );
}

/// **Verification: Batch rejects duplicate object names in single batch**
///
/// This validates the infrastructure that will prevent Alter/Drop from
/// being applied to the same named object twice in one batch.
#[test]
fn test_batch_rejects_duplicate_object_names() {
    let base_version = CatalogVersion::new(1);
    let next_version = CatalogVersion::new(2);

    let proc1 = procedure_contract(300, "test.DupName", next_version);
    let proc2 = procedure_contract(301, "test.DupName", next_version);

    let batch = definition_batch(
        base_version,
        vec![
            DefinitionOperation::Create(CatalogDefinition::Procedure(proc1)),
            DefinitionOperation::Create(CatalogDefinition::Procedure(proc2)),
        ],
    );

    let result = batch.dry_run();
    assert!(result.is_err(), "batch must reject duplicate object names");
    assert!(
        result
            .unwrap_err()
            .message()
            .contains("same lifecycle object name twice"),
        "error must indicate duplicate name issue"
    );
}

/// **Verification: Batch version must advance (non-zero increment)**
///
/// This validates the infrastructure that ensures Alter/Drop always
/// advance the catalog version.
#[test]
fn test_batch_version_advancement_is_monotonic() {
    let base_version = CatalogVersion::new(5);
    let next_version = CatalogVersion::new(6);

    let batch = batch_with_create(base_version, 400, "test.Monotonic", next_version);
    let plan = batch.dry_run().expect("valid batch");

    assert_eq!(
        plan.next_version.get() - plan.previous_version.get(),
        1,
        "catalog version must advance by exactly 1 per batch"
    );

    assert!(
        plan.next_version.get() > plan.previous_version.get(),
        "next version must be greater than previous version"
    );
}

/// **Verification: Empty batch is rejected**
///
/// This validates the infrastructure that ensures batches always have
/// meaningful work (needed for Alter/Drop to be meaningful).
#[test]
fn test_batch_empty_operations_rejected() {
    let base_version = CatalogVersion::new(1);

    let batch = definition_batch(base_version, Vec::new());

    let result = batch.dry_run();
    assert!(result.is_err(), "batch must reject empty operations");
    assert!(
        result
            .unwrap_err()
            .message()
            .contains("at least one operation"),
        "error must indicate empty batch issue"
    );
}

/// **Verification: Batch ID cannot be zero**
///
/// This validates the infrastructure that ensures batch IDs are always
/// meaningful for WAL correlation and recovery.
#[test]
fn test_batch_id_zero_rejected() {
    let base_version = CatalogVersion::new(1);
    let next_version = CatalogVersion::new(2);

    let batch = DefinitionBatch {
        batch_id: DefinitionBatchId::new(0),
        database_id: TEST_DB_ID,
        namespace_id: TEST_NS_ID,
        base_version,
        operations: vec![create_procedure_operation(
            500,
            "test.NoBatchId",
            next_version,
        )],
    };

    let result = batch.dry_run();
    assert!(result.is_err(), "batch must reject batch_id = 0");
    assert!(
        result
            .unwrap_err()
            .message()
            .contains("batch id must not be zero"),
        "error must indicate zero batch ID"
    );
}
