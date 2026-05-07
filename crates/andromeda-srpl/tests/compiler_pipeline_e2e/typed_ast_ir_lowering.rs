use crate::support::*;

#[test]
fn successful_narrow_procedure_compiles_to_ir() {
    let ir = compile_narrow_procedure_signature(
        "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);",
    )
    .unwrap();

    assert_eq!(ir.name.as_catalog_path(), "Inventory.ReserveStock");
    assert_eq!(ir.inputs.len(), 1);
    assert_eq!(ir.inputs[0].name, "ProductId");
    assert_eq!(ir.result_streams.len(), 1);
    assert_eq!(ir.result_streams[0].name, "Reservation");
    assert_eq!(ir.result_streams[0].cardinality, Cardinality::One);
    assert_eq!(ir.result_streams[0].columns[0].name, "Reserved");
    assert!(ir.body.operations.is_empty());
}

#[test]
fn tiny_body_syntax_compiles_to_bounded_operation_ir() {
    let ir = compile_narrow_procedure_signature(
        "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) body { read Inventory.ProductStock Stock one; assert Quantity InsufficientStock; update Inventory.ProductStock AvailableQuantity; emit Reservation (Reserved); }",
    )
    .unwrap();

    assert_eq!(ir.body.operations.len(), 4);
    assert!(ir.body.validate_bounded().is_ok());
    assert!(matches!(
        &ir.body.operations[0].kind,
        SrplBusinessOperationKindIr::Read { binding, .. } if binding.as_str() == "Stock"
    ));
    assert!(matches!(
        &ir.body.operations[1].kind,
        SrplBusinessOperationKindIr::Assert {
            failure_code,
            ..
        } if failure_code.as_str() == "InsufficientStock"
    ));
    assert!(matches!(
        &ir.body.operations[2].kind,
        SrplBusinessOperationKindIr::Update { assignments, .. }
            if matches!(
                assignments.first(),
                Some(assignment) if assignment.field.as_str() == "AvailableQuantity"
            )
    ));
    assert!(matches!(
        &ir.body.operations[3].kind,
        SrplBusinessOperationKindIr::Emit { stream, values }
            if stream.as_str() == "Reservation"
                && matches!(
                    values.first(),
                    Some(value) if value.column.as_str() == "Reserved"
                )
    ));
}

#[test]
fn direct_ir_to_candidate_preserves_catalog_object_identity_inputs() {
    let ir = compile_narrow_procedure_signature(
        "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);",
    )
    .unwrap();
    let candidate = lower_ir_to_contract_candidate(ir, contract_metadata()).unwrap();

    assert_eq!(candidate.object.object_id, CatalogObjectId::new(11));
    assert_eq!(candidate.procedure_id, ProcedureId::new(11));
}

#[test]
fn direct_ir_to_catalog_definition_materializes_procedure_contract() {
    let ir = compile_narrow_procedure_signature(
        "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);",
    )
    .unwrap();
    let definition = lower_ir_to_catalog_definition(ir, contract_metadata()).unwrap();

    let CatalogDefinition::Procedure(contract) = definition else {
        panic!("SRPL procedure IR should lower to a catalog procedure definition");
    };
    assert_eq!(contract.object.kind, ObjectKind::Procedure);
    assert_eq!(
        contract.object.name.as_catalog_path(),
        "Inventory.ReserveStock"
    );
    assert_eq!(contract.procedure_id, ProcedureId::new(11));
    assert_eq!(
        contract.structured_inputs[0].as_catalog_path(),
        "Inventory.StockRequest"
    );
    assert!(contract.validate_canonical_hash().is_ok());
}

#[test]
fn reserve_stock_srpl_source_can_build_catalog_definition_batch() {
    let batch = compile_narrow_procedure_definition_batch(
        INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE,
        inventory_reserve_stock_contract_metadata(CatalogVersion::new(1)),
        INVENTORY_DEFINITION_BATCH_ID,
        INVENTORY_DATABASE_ID,
        INVENTORY_NAMESPACE_ID,
        CatalogVersion::new(0),
    )
    .unwrap();

    let plan = batch.dry_run().unwrap();

    assert_eq!(batch.operations.len(), 1);
    assert_eq!(plan.next_version, CatalogVersion::new(1));
    assert_eq!(plan.created_objects[0].kind, ObjectKind::Procedure);
    let DefinitionOperation::Create(CatalogDefinition::Procedure(contract)) = &batch.operations[0]
    else {
        panic!("SRPL definition batch should create a catalog procedure");
    };
    assert_eq!(
        contract.object.name.as_catalog_path(),
        "Inventory.ReserveStock"
    );
    assert_eq!(
        contract.contract_hash,
        inventory_reserve_stock_contract().unwrap().contract_hash
    );
}

#[test]
fn reserve_stock_body_skeleton_is_typed_and_deterministic() {
    let first = inventory_reserve_stock_body_ir().unwrap();
    let second = inventory_reserve_stock_body_ir().unwrap();

    assert_eq!(first, second);
    assert_eq!(first.operations.len(), 4);
    assert!(first.validate_bounded().is_ok());

    assert!(matches!(
        &first.operations[0].kind,
        SrplBusinessOperationKindIr::Read { .. }
    ));
    assert!(matches!(
        &first.operations[1].kind,
        SrplBusinessOperationKindIr::Assert { .. }
    ));
    let SrplBusinessOperationKindIr::Update { assignments, .. } = &first.operations[2].kind else {
        panic!("ReserveStock operation 2 should be a typed update");
    };
    assert!(matches!(
        &assignments[0].value,
        SrplValueIr::SubtractInput { .. }
    ));
    assert!(matches!(
        &first.operations[3].kind,
        SrplBusinessOperationKindIr::Emit { .. }
    ));
}
