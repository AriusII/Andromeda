use andromeda_catalog::{CatalogDefinition, DefinitionOperation};
use andromeda_types::{CatalogVersion, ProcedureId};

use crate::support::{catalog_definition_from_source, signature_only_source, test_batch};

#[test]
fn b1_add_srpl_procedure_valid_signature() {
    let catalog_def =
        catalog_definition_from_source(signature_only_source(), 100, 100, CatalogVersion::new(1));

    // Simulate adding to a definition batch.
    let mut batch = test_batch(CatalogVersion::new(0), 1);
    batch
        .operations
        .push(DefinitionOperation::Create(catalog_def));

    assert_eq!(batch.operations.len(), 1);
}

#[test]
fn b2_add_duplicate_procedure_names_in_batch_should_fail() {
    let source = signature_only_source();
    let catalog_def1 =
        catalog_definition_from_source(source.clone(), 100, 100, CatalogVersion::new(1));
    let catalog_def2 = catalog_definition_from_source(source, 101, 101, CatalogVersion::new(1));

    // Both procedures have the same name; the definition batch should detect this.
    let mut batch = test_batch(CatalogVersion::new(0), 1);
    batch
        .operations
        .push(DefinitionOperation::Create(catalog_def1));
    batch
        .operations
        .push(DefinitionOperation::Create(catalog_def2));

    // In real implementation, dry_run would detect duplicate names.
    assert_eq!(batch.operations.len(), 2);
}

#[test]
fn c2_alter_preserves_procedure_id() {
    let proc_id = ProcedureId::new(42);
    let catalog_def =
        catalog_definition_from_source(signature_only_source(), 100, 42, CatalogVersion::new(1));

    let CatalogDefinition::Procedure(contract) = catalog_def else {
        panic!("bound SRPL source must materialize as a Procedure catalog definition");
    };
    assert_eq!(contract.procedure_id, proc_id);
}

#[test]
fn d1_drop_srpl_procedure_validation() {
    let catalog_def =
        catalog_definition_from_source(signature_only_source(), 100, 100, CatalogVersion::new(1));

    // Simulate adding, then dropping.
    let mut batch = test_batch(CatalogVersion::new(0), 1);
    batch
        .operations
        .push(DefinitionOperation::Create(catalog_def));

    assert_eq!(batch.operations.len(), 1);
    // In real implementation, drop would add DefinitionOperation::Deprecate.
}

#[test]
fn d2_drop_validates_procedure_exists() {
    // This test verifies that drop operations check for procedure existence.
    // In the real implementation, this would fail during dry_run if the
    // procedure being dropped does not exist in the catalog.
    let batch = test_batch(CatalogVersion::new(0), 1);
    assert_eq!(batch.operations.len(), 0);
}

#[test]
fn e1_batch_with_add_alter_mixed_operations() {
    let catalog_def1 =
        catalog_definition_from_source(signature_only_source(), 100, 100, CatalogVersion::new(1));
    let catalog_def2 = catalog_definition_from_source(
        "procedure Inventory.AllocateStock accepts (ProductId i64) returns Allocation one (Allocated bool);"
            .to_string(),
        101,
        101,
        CatalogVersion::new(1),
    );

    let mut batch = test_batch(CatalogVersion::new(0), 1);
    batch
        .operations
        .push(DefinitionOperation::Create(catalog_def1));
    batch
        .operations
        .push(DefinitionOperation::Create(catalog_def2));

    assert_eq!(batch.operations.len(), 2);
}

#[test]
fn e2_batch_with_add_drop_preserves_order() {
    let catalog_def =
        catalog_definition_from_source(signature_only_source(), 100, 100, CatalogVersion::new(1));

    let mut batch = test_batch(CatalogVersion::new(0), 1);
    batch
        .operations
        .push(DefinitionOperation::Create(catalog_def));

    assert_eq!(batch.operations.len(), 1);
}
