use andromeda_catalog::{
    AccessMode, CatalogSnapshot, CompatibilityPolicy, INVENTORY_DATABASE_ID,
    INVENTORY_NAMESPACE_ID, IsolationPolicy, MultiResultPolicy, ObjectKind, ProcedureErrorPolicy,
    ProtocolLayoutRef, QualifiedName, ResultMetadataPolicy, StatsVersion, TransactionPolicy,
    inventory_domain_definition_batch, inventory_reserve_stock_contract,
    inventory_reserve_stock_contract_candidate,
};
use andromeda_core::{
    AndromedaErrorKind, CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash,
    ProcedureId, ScalarType, TypeDescriptor,
};
use andromeda_srpl::{
    SourceSpan,
    compiler::{
        INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE, bind_executable_procedure_plan,
        compile_inventory_reserve_stock_contract,
        compile_inventory_reserve_stock_contract_candidate,
        compile_narrow_procedure_contract_candidate, compile_narrow_procedure_signature,
        inventory_reserve_stock_body_ir, inventory_reserve_stock_contract_metadata,
        lower_ir_to_contract_candidate, parse_procedure_signature,
    },
    diagnostics::DiagnosticPhase,
    model::{
        Cardinality, ProcedureSignature, ResultContract, SrplBusinessOperationKindIr,
        SrplProcedureContractMetadata, SrplProcedureIr, SrplValueIr,
    },
    source::SrplSource,
};

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
fn parser_diagnostics_include_phase_and_span() {
    let diagnostic =
        parse_procedure_signature("procedure X accepts () returns R many ();").unwrap_err();

    assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
    assert!(diagnostic.location.is_some());
    assert!(diagnostic.location.unwrap().is_valid());
    assert!(diagnostic.message.contains("at least one column"));
}

#[test]
fn forbidden_constructs_reject_before_lowering() {
    let diagnostic = compile_narrow_procedure_signature(
        "procedure X accepts () returns R many (C bool); execute sql",
    )
    .unwrap_err();

    assert_eq!(diagnostic.phase, DiagnosticPhase::Binding);
    assert!(diagnostic.location.is_some());
    assert!(diagnostic.message.contains("dynamic text SQL"));
}

#[test]
fn duplicate_parameter_names_reject_with_binding_span() {
    let diagnostic = compile_narrow_procedure_signature(
        "procedure Inventory.ReserveStock accepts (ProductId i64, ProductId i64) returns Reservation one (Reserved bool);",
    )
        .unwrap_err();

    assert_eq!(diagnostic.phase, DiagnosticPhase::Binding);
    assert!(diagnostic.location.is_some());
    assert!(diagnostic.message.contains("input names must be unique"));
}

#[test]
fn duplicate_result_names_reject_in_public_contract_model() {
    let signature = ProcedureSignature {
        name: QualifiedName::parse("Inventory.ReserveStock").unwrap(),
        accepts: Vec::new(),
        returns: vec![
            ResultContract {
                name: "Reservation".to_string(),
                cardinality: Cardinality::One,
                columns: vec![ColumnDescriptor {
                    name: "Reserved".to_string(),
                    data_type: TypeDescriptor::required(ScalarType::Bool),
                    ordinal: 0,
                }],
            },
            ResultContract {
                name: "Reservation".to_string(),
                cardinality: Cardinality::OptionalOne,
                columns: vec![ColumnDescriptor {
                    name: "AlreadyReserved".to_string(),
                    data_type: TypeDescriptor::required(ScalarType::Bool),
                    ordinal: 0,
                }],
            },
        ],
    };

    let error = signature.validate().unwrap_err();

    assert!(
        error
            .message()
            .contains("result stream names must be unique")
    );
}

#[test]
fn public_api_is_consumable_from_outside_the_crate() {
    let source = SrplSource::new("procedure X accepts () returns R many (C bool);");
    assert!(source.forbidden_construct_diagnostics().is_empty());

    let span = SourceSpan::new(0, source.text.len());
    assert!(span.is_valid());

    let ir: SrplProcedureIr = andromeda_srpl::compile_narrow_procedure_signature(source.text)
        .expect("root re-export remains public");
    assert_eq!(
        ir.result_streams[0].cardinality,
        andromeda_srpl::Cardinality::Many
    );
}

fn contract_metadata() -> SrplProcedureContractMetadata {
    SrplProcedureContractMetadata {
        object_id: CatalogObjectId::new(11),
        procedure_id: ProcedureId::new(11),
        catalog_version: CatalogVersion::new(3),
        stats_version: StatsVersion::new(1),
        protocol_layout: ProtocolLayoutRef {
            descriptor_set_hash: ContractHash::test_vector(0xA1),
            frame_envelope_hash: ContractHash::test_vector(0xA2),
        },
        structured_inputs: vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
        required_permissions: vec!["Inventory.ReserveStock.Execute".to_string()],
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadWrite,
            isolation: IsolationPolicy::Serializable,
            retryable: false,
        },
        compatibility_policy: CompatibilityPolicy::ExactHash,
        result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
        error_policy: ProcedureErrorPolicy {
            rollback_on_error: true,
            allowed_error_codes: vec!["InsufficientStock".to_string()],
        },
        multi_result_policy: MultiResultPolicy::SingleResultOnly,
    }
}

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
    assert!(contract.result_streams[0].row_count_exact_required);
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

fn inventory_catalog_snapshot() -> CatalogSnapshot {
    let batch = inventory_domain_definition_batch().unwrap();
    let plan = batch.dry_run().unwrap();
    let mut snapshot = CatalogSnapshot::empty(
        INVENTORY_DATABASE_ID,
        INVENTORY_NAMESPACE_ID,
        batch.base_version,
    );
    snapshot.apply_mutation_plan(&plan.mutation_plan).unwrap();
    snapshot
}

#[test]
fn inventory_reserve_stock_body_binds_to_deterministic_executable_plan() {
    let mut ir = compile_narrow_procedure_signature(
        "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool);",
    )
    .unwrap();
    ir.body = inventory_reserve_stock_body_ir().unwrap();
    let snapshot = inventory_catalog_snapshot();

    let first = bind_executable_procedure_plan(&ir, &snapshot).unwrap();
    let second = bind_executable_procedure_plan(&ir, &snapshot).unwrap();

    assert_eq!(first, second);
    assert_eq!(first.body.operations.len(), 4);
    assert_eq!(first.evidence.catalog_version, snapshot.version);
    assert_eq!(
        first.evidence.procedure_object.name.as_catalog_path(),
        "Inventory.ReserveStock"
    );
    assert!(!first.evidence.procedure_contract.contract_hash.is_zero());
    assert_eq!(
        first.evidence.stock_object.object.name.as_catalog_path(),
        "Inventory.ProductStock"
    );
    assert!(!first.evidence.stock_object.shape_hash.is_zero());
    assert_eq!(
        first
            .evidence
            .reservation_object
            .object
            .name
            .as_catalog_path(),
        "Inventory.Reservation"
    );
    assert!(first.validate().is_ok());
}

#[test]
fn pdf_style_inventory_source_binds_to_deterministic_executable_plan() {
    let ir = compile_narrow_procedure_signature(INVENTORY_RESERVE_STOCK_PDF_STYLE_SOURCE).unwrap();
    let snapshot = inventory_catalog_snapshot();

    let plan = bind_executable_procedure_plan(&ir, &snapshot).unwrap();

    assert_eq!(plan.body.operations.len(), 4);
    assert!(matches!(
        &plan.body.operations[2],
        andromeda_srpl::model::BoundSrplOperationPlan::UpdateTable {
            affected_rows_exact: Some(1),
            ..
        }
    ));
    assert!(plan.validate().is_ok());
}

#[test]
fn executable_plan_rejects_unbound_inventory_catalog_objects() {
    let mut ir = compile_narrow_procedure_signature(
        "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool);",
    )
    .unwrap();
    ir.body = inventory_reserve_stock_body_ir().unwrap();
    let snapshot = CatalogSnapshot::empty(
        INVENTORY_DATABASE_ID,
        INVENTORY_NAMESPACE_ID,
        CatalogVersion::new(1),
    );

    let error = bind_executable_procedure_plan(&ir, &snapshot).unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("missing procedure contract"));
}

#[test]
fn executable_plan_rejects_stale_catalog_version_evidence() {
    let mut ir = compile_narrow_procedure_signature(
        "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool);",
    )
    .unwrap();
    ir.body = inventory_reserve_stock_body_ir().unwrap();
    let mut snapshot = inventory_catalog_snapshot();
    snapshot.version = CatalogVersion::new(2);

    let error = bind_executable_procedure_plan(&ir, &snapshot).unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("active catalog version"));
}

#[test]
fn executable_plan_rejects_unsupported_operation_order_and_cardinality() {
    let mut ir = compile_narrow_procedure_signature(
        "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool);",
    )
    .unwrap();
    ir.body = inventory_reserve_stock_body_ir().unwrap();
    ir.body.operations.swap(1, 2);
    ir.body.operations[1].ordinal = 1;
    ir.body.operations[2].ordinal = 2;
    let snapshot = inventory_catalog_snapshot();

    let error = bind_executable_procedure_plan(&ir, &snapshot).unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
    assert!(error.message().contains("operation order"));
}
