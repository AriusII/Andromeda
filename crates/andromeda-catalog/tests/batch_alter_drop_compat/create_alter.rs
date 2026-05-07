use super::common::*;

/// **Verification of Create semantics (prerequisite for Alter)**
///
/// When a Procedure is created:
/// - The object's catalog_version must match the planned next_version
/// - The ContractHash must be present and non-zero
/// - After dry_run completes, the next_version is incremented
/// - The created object is visible in the plan
///
/// This test verifies the infrastructure that Alter will depend on.
#[test]
fn test_batch_create_procedure_generates_correct_version() {
    let base_version = CatalogVersion::new(1);
    let next_version = CatalogVersion::new(2);

    let batch = batch_with_create(base_version, 100, "test.Proc1", next_version);

    assert_eq!(batch.base_version, base_version);
    assert_eq!(batch.operations.len(), 1);

    let plan = batch.dry_run().expect("valid batch plan");

    assert_eq!(plan.previous_version, base_version);
    assert_eq!(plan.next_version, next_version);

    assert_eq!(plan.created_objects.len(), 1);
    let created = &plan.created_objects[0];
    assert_eq!(created.object_id, CatalogObjectId::new(100));
    assert_eq!(created.name, QualifiedName::parse("test.Proc1").unwrap());
    assert_eq!(created.kind, ObjectKind::Procedure);
    assert_eq!(created.planned_version, next_version);

    assert_eq!(plan.deprecated_objects.len(), 0);

    assert_eq!(plan.mutation_plan.deltas.len(), 1);
    let delta = &plan.mutation_plan.deltas[0];
    assert_eq!(delta.planned_version, next_version);

    let definition = delta
        .definition()
        .expect("create operation must have definition");
    if let CatalogDefinition::Procedure(contract) = definition {
        assert_eq!(contract.object.catalog_version, next_version);
        assert_eq!(contract.contract_hash, contract.canonical_hash());
        assert!(!contract.contract_hash.is_zero());
        assert_eq!(contract.object.object_id, CatalogObjectId::new(100));
    } else {
        panic!("Expected Procedure definition");
    }
}

/// **Verification of Alter infrastructure compatibility (DEC-022)**
///
/// When a Procedure is altered:
/// - The same `ProcedureId` and `QualifiedName` must be preserved
/// - A canonical `ContractHash` must be materialized from the typed contract
/// - The `CatalogVersion` must be incremented to the next_version
/// - The old `ContractHash` must be retained in catalog history
///
/// This test verifies that the batch infrastructure can support future Alter by:
/// 1. Creating a procedure at version N
/// 2. Simulating Alter by creating a replacement procedure at version N+1
///    (Note: Real Alter would modify same ID; here we use Create to test infrastructure)
/// 3. Verifying that identity, version advancement, and hash tracking work correctly
///
/// Once Alter is implemented, this test will be replaced with actual Alter operations.
#[test]
fn test_batch_alter_procedure_semantics_preserves_id_increments_version() {
    let base_v1 = CatalogVersion::new(1);
    let v2 = CatalogVersion::new(2);

    let batch1 = batch_with_create(base_v1, 100, "test.ProcX", v2);
    let plan1 = batch1.dry_run().expect("first batch valid");

    assert_eq!(plan1.created_objects.len(), 1);
    assert_eq!(plan1.next_version, v2);

    let original_contract = plan1.mutation_plan.deltas[0]
        .definition()
        .expect("has definition");
    let (original_procedure_id, original_name, original_object_id, original_hash) =
        if let CatalogDefinition::Procedure(contract) = original_contract {
            (
                contract.procedure_id,
                contract.object.name.clone(),
                contract.object.object_id,
                contract.contract_hash,
            )
        } else {
            panic!("expected procedure");
        };
    assert!(!original_hash.is_zero());

    let v3 = CatalogVersion::new(3);

    let batch2 = batch_with_create(v2, 100, "test.ProcX", v3);
    let plan2 = batch2.dry_run().expect("second batch valid");

    assert_eq!(plan2.previous_version, v2);
    assert_eq!(plan2.next_version, v3);
    assert_eq!(plan2.created_objects.len(), 1);

    assert!(plan2.next_version.get() > plan1.next_version.get());

    let new_contract = plan2.mutation_plan.deltas[0]
        .definition()
        .expect("has definition");
    let (new_procedure_id, new_name, new_object_id, new_hash) =
        if let CatalogDefinition::Procedure(contract) = new_contract {
            (
                contract.procedure_id,
                contract.object.name.clone(),
                contract.object.object_id,
                contract.contract_hash,
            )
        } else {
            panic!("expected procedure");
        };

    assert_eq!(new_procedure_id, original_procedure_id);
    assert_eq!(new_procedure_id, ProcedureId::new(100));
    assert_eq!(new_name, original_name);
    assert_eq!(new_object_id, original_object_id);
    assert_eq!(new_hash, original_hash);

    assert_eq!(
        plan2.next_version.get() - plan1.next_version.get(),
        1,
        "version advancement must be exactly 1 per batch"
    );
}
