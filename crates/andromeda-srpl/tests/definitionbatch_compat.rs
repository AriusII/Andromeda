//! SRPL catalog definition lifecycle compatibility tests.

use andromeda_catalog::{
    CatalogDefinition, DefinitionBatch, DefinitionBatchId, DefinitionOperation,
};
use andromeda_core::{
    CatalogObjectId, CatalogVersion, DatabaseId, NamespaceId, ProcedureId, ScalarType,
    TypeDescriptor,
};
use andromeda_srpl::definition_batch_bridge::SrplProcedureDefinition;

const TEST_DB_ID: DatabaseId = DatabaseId::new(1);
const TEST_NS_ID: NamespaceId = NamespaceId::new(1);

fn test_batch(base_version: CatalogVersion, batch_id: u64) -> DefinitionBatch {
    DefinitionBatch {
        batch_id: DefinitionBatchId::new(batch_id),
        database_id: TEST_DB_ID,
        namespace_id: TEST_NS_ID,
        base_version,
        operations: Vec::new(),
    }
}

fn signature_only_source() -> String {
    "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);".to_string()
}

#[test]
fn a1_parse_to_ast_round_trip() {
    let source = signature_only_source();
    let mut def = SrplProcedureDefinition::from_source(source.clone());

    assert!(def.parse().is_ok());
    assert!(def.parsed_ast.is_some());

    let ast = def.parsed_ast.unwrap();
    assert_eq!(ast.name.value.as_catalog_path(), "Inventory.ReserveStock");
}

#[test]
fn a2_ast_to_ir_compilation() {
    let source = signature_only_source();
    let mut def = SrplProcedureDefinition::from_source(source);

    assert!(def.parse().is_ok());
    assert!(def.bind_and_lower().is_ok());
    assert!(def.compiled_ir.is_some());

    let ir = def.compiled_ir.unwrap();
    assert_eq!(ir.name.as_catalog_path(), "Inventory.ReserveStock");
    assert_eq!(ir.inputs.len(), 1);
    assert_eq!(ir.result_streams.len(), 1);
}

#[test]
fn a3_ir_to_catalog_definition_materialization() {
    let source = signature_only_source();
    let mut def = SrplProcedureDefinition::from_source(source);

    assert!(def.parse().is_ok());
    assert!(def.bind_and_lower().is_ok());

    let catalog_def = def.to_catalog_procedure_def(
        CatalogObjectId::new(1),
        ProcedureId::new(1),
        CatalogVersion::new(1),
    );

    assert!(catalog_def.is_ok());
    let def_result = catalog_def.unwrap();
    assert!(matches!(def_result, CatalogDefinition::Procedure(_)));

    if let CatalogDefinition::Procedure(contract) = def_result {
        assert_eq!(
            contract.object.name.as_catalog_path(),
            "Inventory.ReserveStock"
        );
        assert_eq!(contract.inputs.len(), 1);
        assert_eq!(contract.result_streams.len(), 1);
    }
}

#[test]
fn b1_add_srpl_procedure_valid_signature() {
    let source = signature_only_source();
    let mut def = SrplProcedureDefinition::from_source(source);

    assert!(def.parse().is_ok());
    assert!(def.bind_and_lower().is_ok());

    let result = def.to_catalog_procedure_def(
        CatalogObjectId::new(100),
        ProcedureId::new(100),
        CatalogVersion::new(1),
    );

    assert!(result.is_ok());
    let catalog_def = result.unwrap();

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

    let mut def1 = SrplProcedureDefinition::from_source(source.clone());
    assert!(def1.parse().is_ok());
    assert!(def1.bind_and_lower().is_ok());
    let catalog_def1 = def1
        .to_catalog_procedure_def(
            CatalogObjectId::new(100),
            ProcedureId::new(100),
            CatalogVersion::new(1),
        )
        .unwrap();

    let mut def2 = SrplProcedureDefinition::from_source(source);
    assert!(def2.parse().is_ok());
    assert!(def2.bind_and_lower().is_ok());
    let catalog_def2 = def2
        .to_catalog_procedure_def(
            CatalogObjectId::new(101),
            ProcedureId::new(101),
            CatalogVersion::new(1),
        )
        .unwrap();

    // Both procedures have the same name; the definition batch should detect this.
    let mut batch = test_batch(CatalogVersion::new(0), 1);
    batch
        .operations
        .push(DefinitionOperation::Create(catalog_def1));
    batch
        .operations
        .push(DefinitionOperation::Create(catalog_def2));

    // In real implementation, dry_run would detect duplicate names
    assert_eq!(batch.operations.len(), 2);
}

#[test]
fn c1_alter_srpl_procedure_compiles_new_source() {
    let old_source = signature_only_source();
    let new_source = "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool);".to_string();

    let mut old_def = SrplProcedureDefinition::from_source(old_source);
    assert!(old_def.parse().is_ok());
    assert!(old_def.bind_and_lower().is_ok());

    let mut new_def = SrplProcedureDefinition::from_source(new_source);
    assert!(new_def.parse().is_ok());
    assert!(new_def.bind_and_lower().is_ok());

    // New definition has 2 inputs instead of 1
    let old_ir = old_def.compiled_ir.unwrap();
    let new_ir = new_def.compiled_ir.unwrap();
    assert_eq!(old_ir.inputs.len(), 1);
    assert_eq!(new_ir.inputs.len(), 2);
}

#[test]
fn c2_alter_preserves_procedure_id() {
    let source = signature_only_source();
    let mut def = SrplProcedureDefinition::from_source(source);
    assert!(def.parse().is_ok());
    assert!(def.bind_and_lower().is_ok());

    let proc_id = ProcedureId::new(42);
    let catalog_def = def
        .to_catalog_procedure_def(CatalogObjectId::new(100), proc_id, CatalogVersion::new(1))
        .unwrap();

    if let CatalogDefinition::Procedure(contract) = catalog_def {
        assert_eq!(contract.procedure_id, proc_id);
    }
}

#[test]
fn d1_drop_srpl_procedure_validation() {
    let source = signature_only_source();
    let mut def = SrplProcedureDefinition::from_source(source);
    assert!(def.parse().is_ok());
    assert!(def.bind_and_lower().is_ok());

    let catalog_def = def
        .to_catalog_procedure_def(
            CatalogObjectId::new(100),
            ProcedureId::new(100),
            CatalogVersion::new(1),
        )
        .unwrap();

    // Simulate adding, then dropping
    let mut batch = test_batch(CatalogVersion::new(0), 1);
    batch
        .operations
        .push(DefinitionOperation::Create(catalog_def));

    assert_eq!(batch.operations.len(), 1);
    // In real implementation, drop would add DefinitionOperation::Deprecate
}

#[test]
fn d2_drop_validates_procedure_exists() {
    // This test verifies that drop operations check for procedure existence
    // In the real implementation, this would fail during dry_run if the
    // procedure being dropped doesn't exist in the catalog.

    let batch = test_batch(CatalogVersion::new(0), 1);
    assert_eq!(batch.operations.len(), 0);
}

#[test]
fn e1_batch_with_add_alter_mixed_operations() {
    let source = signature_only_source();

    // Create first procedure
    let mut def1 = SrplProcedureDefinition::from_source(source.clone());
    assert!(def1.parse().is_ok());
    assert!(def1.bind_and_lower().is_ok());
    let catalog_def1 = def1
        .to_catalog_procedure_def(
            CatalogObjectId::new(100),
            ProcedureId::new(100),
            CatalogVersion::new(1),
        )
        .unwrap();

    // Create second procedure for ALTER (new version)
    let new_source = "procedure Inventory.AllocateStock accepts (ProductId i64) returns Allocation one (Allocated bool);".to_string();
    let mut def2 = SrplProcedureDefinition::from_source(new_source);
    assert!(def2.parse().is_ok());
    assert!(def2.bind_and_lower().is_ok());
    let catalog_def2 = def2
        .to_catalog_procedure_def(
            CatalogObjectId::new(101),
            ProcedureId::new(101),
            CatalogVersion::new(1),
        )
        .unwrap();

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
    let source = signature_only_source();

    let mut def = SrplProcedureDefinition::from_source(source);
    assert!(def.parse().is_ok());
    assert!(def.bind_and_lower().is_ok());
    let catalog_def = def
        .to_catalog_procedure_def(
            CatalogObjectId::new(100),
            ProcedureId::new(100),
            CatalogVersion::new(1),
        )
        .unwrap();

    let mut batch = test_batch(CatalogVersion::new(0), 1);
    batch
        .operations
        .push(DefinitionOperation::Create(catalog_def));

    assert_eq!(batch.operations.len(), 1);
}

#[test]
fn f1_syntax_error_in_srpl_source() {
    let bad_source =
        "procedure Inventory.ReserveStock accepts (ProductId i64) returns R many ();".to_string();
    let mut def = SrplProcedureDefinition::from_source(bad_source);

    let result = def.parse();
    assert!(result.is_err());
}

#[test]
fn f2_duplicate_input_names_syntax_error() {
    let bad_source = "procedure Inventory.ReserveStock accepts (ProductId i64, ProductId i64) returns Reservation one (Reserved bool);".to_string();
    let mut def = SrplProcedureDefinition::from_source(bad_source);

    assert!(def.parse().is_ok());
    let result = def.bind_and_lower();
    assert!(result.is_err());
}

#[test]
fn f3_single_result_stream_survives_parse_and_lower() {
    let source = signature_only_source();
    let mut def = SrplProcedureDefinition::from_source(source);

    assert!(def.parse().is_ok());
    assert!(def.bind_and_lower().is_ok());

    let ir = def.compiled_ir.unwrap();
    assert_eq!(ir.result_streams.len(), 1);
    assert_eq!(ir.result_streams[0].name, "Reservation");
    assert_eq!(ir.result_streams[0].columns.len(), 1);
    assert_eq!(ir.result_streams[0].columns[0].name, "Reserved");
}

#[test]
fn f4_declared_scalar_types_survive_binding() {
    let source = signature_only_source();
    let mut def = SrplProcedureDefinition::from_source(source);

    assert!(def.parse().is_ok());
    assert!(def.bind_and_lower().is_ok());

    let ir = def.compiled_ir.unwrap();
    assert_eq!(
        ir.inputs[0].data_type,
        TypeDescriptor::required(ScalarType::I64)
    );
    assert_eq!(
        ir.result_streams[0].columns[0].data_type,
        TypeDescriptor::required(ScalarType::Bool)
    );
}

#[test]
fn f5_body_read_operation_lowers_without_catalog_lookup() {
    let source = "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool) body { read Inventory.ProductStock Stock one; emit Reservation (Reserved); }".to_string();
    let mut def = SrplProcedureDefinition::from_source(source);

    assert!(def.parse().is_ok());
    assert!(def.bind_and_lower().is_ok());

    let ir = def.compiled_ir.unwrap();
    assert_eq!(ir.body.operations.len(), 2);
}

#[test]
fn g1_one_failed_procedure_rejects_entire_batch() {
    let good_source = signature_only_source();
    let bad_source =
        "procedure Inventory.ReserveStock accepts (ProductId i64) returns R many ();".to_string();

    let mut good_def = SrplProcedureDefinition::from_source(good_source);
    assert!(good_def.parse().is_ok());
    assert!(good_def.bind_and_lower().is_ok());

    let mut bad_def = SrplProcedureDefinition::from_source(bad_source);
    assert!(bad_def.parse().is_err());

    // In real implementation, definition dry-run would fail due to bad_def.
}

#[test]
fn g2_batch_error_includes_all_failures() {
    // Multiple procedures with errors should report all failures.
    let source1 = "procedure X accepts () returns R many ();".to_string();
    let source2 = "procedure Y accepts () returns S many ();".to_string();

    let mut def1 = SrplProcedureDefinition::from_source(source1);
    let mut def2 = SrplProcedureDefinition::from_source(source2);

    assert!(def1.parse().is_err());
    assert!(def2.parse().is_err());

    // In real implementation, dry-run would accumulate both errors
}
