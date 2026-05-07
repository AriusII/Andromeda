use crate::support::*;

#[test]
fn inventory_contract_metadata_is_derived_from_catalog_fixture() {
    let metadata = inventory_reserve_stock_contract_metadata(CatalogVersion::new(1));
    let fixture = inventory_reserve_stock_contract_candidate(CatalogVersion::new(1));

    assert_eq!(metadata.object_id, fixture.object.object_id);
    assert_eq!(metadata.procedure_id, fixture.procedure_id);
    assert_eq!(metadata.catalog_version, fixture.object.catalog_version);
    assert_eq!(metadata.stats_version, fixture.stats_version);
    assert_eq!(metadata.protocol_layout, fixture.protocol_layout);
    assert_eq!(metadata.structured_inputs, fixture.structured_inputs);
    assert_eq!(metadata.required_permissions, fixture.required_permissions);
    assert_eq!(metadata.transaction_policy, fixture.transaction_policy);
    assert_eq!(metadata.compatibility_policy, fixture.compatibility_policy);
    assert_eq!(
        metadata.result_metadata_policy,
        fixture.result_metadata_policy
    );
    assert_eq!(metadata.error_policy, fixture.error_policy);
    assert_eq!(metadata.multi_result_policy, fixture.multi_result_policy);
}

#[test]
fn pdf_style_inventory_source_materializes_catalog_fixture_contract() {
    let from_srpl = compile_inventory_reserve_stock_contract(CatalogVersion::new(1)).unwrap();
    let fixture = inventory_reserve_stock_contract().unwrap();

    assert_eq!(from_srpl, fixture);
    assert_eq!(from_srpl.contract_hash, fixture.contract_hash);
    assert_eq!(from_srpl.contract_hash, from_srpl.canonical_hash());
}

#[test]
fn pdf_style_inventory_candidate_matches_fixture_hash_golden() {
    let from_srpl = compile_inventory_reserve_stock_contract_candidate(CatalogVersion::new(1))
        .unwrap()
        .materialize()
        .unwrap();
    let fixture = inventory_reserve_stock_contract().unwrap();

    assert_eq!(from_srpl.as_ref(), fixture.as_ref());
    assert_eq!(from_srpl.contract_hash, fixture.contract_hash);
}

#[test]
fn srpl_ir_lowers_to_catalog_contract_candidate_with_cardinality_mapping() {
    let candidate = compile_narrow_procedure_contract_candidate(
        "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation optional_one (Reserved bool);",
        contract_metadata(),
    )
    .unwrap();
    let contract = candidate.materialize().unwrap();

    assert_eq!(contract.object.kind, ObjectKind::Procedure);
    assert_eq!(
        contract.object.name.as_catalog_path(),
        "Inventory.ReserveStock"
    );
    assert_eq!(contract.inputs[0].name, "ProductId");
    assert_eq!(
        contract.structured_inputs[0].as_catalog_path(),
        "Inventory.StockRequest"
    );
    assert_eq!(contract.result_streams[0].name, "Reservation");
    assert_eq!(
        contract.result_streams[0].cardinality,
        ResultStreamCardinality::OptionalOne
    );
    assert!(!contract.result_streams[0].row_count_exact_required);
    assert!(contract.validate_canonical_hash().is_ok());
}

#[test]
fn contract_candidate_hash_is_stable_for_identical_srpl_and_metadata() {
    let source = "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation many (Reserved bool);";
    let first = compile_narrow_procedure_contract_candidate(source, contract_metadata())
        .unwrap()
        .materialize()
        .unwrap();
    let second = compile_narrow_procedure_contract_candidate(source, contract_metadata())
        .unwrap()
        .materialize()
        .unwrap();

    assert_eq!(first.contract_hash, second.contract_hash);
    assert!(!first.contract_hash.is_zero());
    assert_eq!(
        first.result_streams[0].cardinality,
        ResultStreamCardinality::Many
    );
    assert!(!first.result_streams[0].row_count_exact_required);
}

#[test]
fn contract_candidate_preserves_v0_metadata_and_error_policy() {
    let contract = compile_narrow_procedure_contract_candidate(
        INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE,
        contract_metadata(),
    )
    .unwrap()
    .materialize()
    .unwrap();

    assert_eq!(contract.stats_version, StatsVersion::new(1));
    assert_eq!(
        contract.result_metadata_policy,
        ResultMetadataPolicy::RequireBeforePayload
    );
    assert_eq!(
        contract.error_policy.allowed_error_codes,
        vec!["InsufficientStock".to_string()]
    );
    assert_eq!(
        contract.multi_result_policy,
        MultiResultPolicy::SingleResultOnly
    );
    assert_eq!(contract.result_streams[0].stream_id, 1);
    assert!(contract.validate_canonical_hash().is_ok());
}

#[test]
fn contract_candidate_rejects_invalid_metadata_before_catalog_publication() {
    let mut metadata = contract_metadata();
    metadata.required_permissions.clear();
    let diagnostic = compile_narrow_procedure_contract_candidate(
        "procedure Inventory.ReserveStock accepts () returns Reservation one (Reserved bool);",
        metadata,
    )
    .unwrap_err();

    assert_eq!(diagnostic.phase, DiagnosticPhase::IrLowering);
    assert!(diagnostic.message.contains("permissions"));
}

#[test]
fn contract_candidate_rejects_undeclared_srpl_error_policy_before_publication() {
    let mut metadata = inventory_reserve_stock_contract_metadata(CatalogVersion::new(1));
    metadata.error_policy.allowed_error_codes.clear();

    let diagnostic = compile_narrow_procedure_contract_candidate(
        INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE,
        metadata,
    )
    .unwrap_err();

    assert_eq!(diagnostic.phase, DiagnosticPhase::IrLowering);
    assert!(diagnostic.message.contains("error code"));
    assert!(diagnostic.message.contains("not declared"));
}

#[test]
fn srpl_catalog_definition_batch_validates_structured_input_dependencies() {
    let base_version = CatalogVersion::new(0);
    let next_version = CatalogVersion::new(1);
    let mut metadata = inventory_reserve_stock_contract_metadata(next_version);
    metadata.structured_inputs = vec![QualifiedName::parse("Inventory.StockRequest").unwrap()];
    let procedure_definition =
        compile_narrow_procedure_definition(INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE, metadata)
            .unwrap();
    let structured_definition =
        CatalogDefinition::StructuredObject(stock_request_structured_object(next_version));
    let missing_dependency_batch = DefinitionBatch {
        batch_id: DefinitionBatchId::new(0x51_00),
        database_id: INVENTORY_DATABASE_ID,
        namespace_id: INVENTORY_NAMESPACE_ID,
        base_version,
        operations: vec![DefinitionOperation::Create(procedure_definition.clone())],
    };
    let snapshot =
        CatalogSnapshot::empty(INVENTORY_DATABASE_ID, INVENTORY_NAMESPACE_ID, base_version);
    let missing_dependency_error = snapshot
        .plan_definition_batch(&missing_dependency_batch)
        .unwrap_err();

    assert_eq!(missing_dependency_error.kind(), AndromedaErrorKind::Catalog);
    assert!(missing_dependency_error.message().contains("dependency"));
    assert!(missing_dependency_error.message().contains("missing"));

    let batch = DefinitionBatch {
        batch_id: DefinitionBatchId::new(0x51_01),
        database_id: INVENTORY_DATABASE_ID,
        namespace_id: INVENTORY_NAMESPACE_ID,
        base_version,
        operations: vec![
            DefinitionOperation::Create(structured_definition.clone()),
            DefinitionOperation::Create(procedure_definition.clone()),
        ],
    };
    let plan = batch.dry_run().unwrap();

    assert_eq!(plan.created_objects.len(), 2);
    assert_eq!(plan.created_objects[0].kind, ObjectKind::StructuredObject);
    assert_eq!(plan.created_objects[1].kind, ObjectKind::Procedure);

    let reversed = DefinitionBatch {
        batch_id: DefinitionBatchId::new(0x51_02),
        database_id: INVENTORY_DATABASE_ID,
        namespace_id: INVENTORY_NAMESPACE_ID,
        base_version,
        operations: vec![
            DefinitionOperation::Create(procedure_definition),
            DefinitionOperation::Create(structured_definition),
        ],
    };
    let error = reversed.dry_run().unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("dependencies before dependent"));
}
