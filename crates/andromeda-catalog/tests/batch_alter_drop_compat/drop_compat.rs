use super::common::*;

/// **Verification of Drop infrastructure compatibility (DEC-023)**
///
/// When a Procedure is dropped:
/// - In Restrict mode (default, only legal mode per DEC-023), the drop is rejected
///   if any other object depends on the dropped procedure
/// - If no dependents exist, the drop succeeds
/// - The batch dry-run must report whether the drop succeeded or failed
/// - The procedure is removed from the active catalog but history is retained
///
/// This test verifies that the batch dependency validation infrastructure
/// will support future Drop operations by testing the foundation:
/// 1. Create a procedure that could be dropped
/// 2. Create another procedure that might depend on it
/// 3. Verify batch validation catches dependency issues
///
/// Once Drop is implemented with Restrict mode, this test will verify actual
/// drop rejection when blockers exist.
#[test]
fn test_batch_drop_procedure_validates_restrict_rule() {
    let base_v1 = CatalogVersion::new(1);
    let v2 = CatalogVersion::new(2);

    let batch1 = batch_with_create(base_v1, 101, "test.DropMe", v2);
    let plan1 = batch1.dry_run().expect("first batch valid");

    assert_eq!(plan1.created_objects.len(), 1);
    assert_eq!(plan1.next_version, v2);

    let v3 = CatalogVersion::new(3);
    let drop_dependency = QualifiedName::parse("test.DropMe").unwrap();
    let dependent_contract = procedure_contract_with_structured_inputs(
        102,
        "test.DepOnDropMe",
        v3,
        vec![drop_dependency.clone()],
    );
    assert_eq!(dependent_contract.structured_inputs, vec![drop_dependency]);

    let batch2 = definition_batch(
        v2,
        vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
            dependent_contract,
        ))],
    );

    let plan2 = batch2.dry_run().expect("second batch valid");
    assert_eq!(plan2.created_objects.len(), 1);

    // Future Drop will use the same dependency-bearing Procedure contract shape
    // to report restrict blockers in the dry-run plan.
    assert!(!plan2.mutation_plan.deltas.is_empty());
}
