use crate::support::*;

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
fn executable_plan_validates_result_emit_counts_by_cardinality() {
    let (optional_absent, optional_snapshot) = cardinality_probe_snapshot(
        "Inventory.OptionalProbe",
        "MaybeValue",
        Cardinality::OptionalOne,
        0,
    );
    let optional_plan = bind_executable_procedure_plan(&optional_absent, &optional_snapshot)
        .expect("optional one may represent absence without an emit");
    assert!(matches!(
        optional_plan.body.operations[0],
        andromeda_srpl_ir::BoundSrplOperationPlan::Raise { .. }
    ));

    let (optional_duplicate, optional_duplicate_snapshot) = cardinality_probe_snapshot(
        "Inventory.OptionalProbeDuplicate",
        "MaybeValue",
        Cardinality::OptionalOne,
        2,
    );
    let optional_error =
        bind_executable_procedure_plan(&optional_duplicate, &optional_duplicate_snapshot)
            .expect_err("optional one must reject more than one emitted row shape");
    assert_eq!(optional_error.kind(), AndromedaErrorKind::Srpl);
    assert!(
        optional_error
            .message()
            .contains("optional-one result stream may have zero or one emit operation")
    );

    let (nonempty_absent, nonempty_snapshot) = cardinality_probe_snapshot(
        "Inventory.NonEmptyProbe",
        "Values",
        Cardinality::NonEmptyMany,
        0,
    );
    let nonempty_error = bind_executable_procedure_plan(&nonempty_absent, &nonempty_snapshot)
        .expect_err("nonempty many must reject missing emit coverage");
    assert_eq!(nonempty_error.kind(), AndromedaErrorKind::Srpl);
    assert!(
        nonempty_error
            .message()
            .contains("nonempty-many result stream must have at least one emit operation")
    );

    let (one_absent, one_snapshot) =
        cardinality_probe_snapshot("Inventory.OneProbe", "Value", Cardinality::One, 0);
    let one_error = bind_executable_procedure_plan(&one_absent, &one_snapshot)
        .expect_err("one must reject missing emit coverage");
    assert_eq!(one_error.kind(), AndromedaErrorKind::Srpl);
    assert!(
        one_error
            .message()
            .contains("one result stream must have exactly one emit operation")
    );
}

#[test]
fn executable_plan_resolves_table_names_from_catalog_not_inputs() {
    let ir = compile_narrow_procedure_signature(
        "procedure Inventory.DynamicTableProbe accepts (TableName text(64)) returns R one (C bool) body { read TableName Row one; emit R (C); }",
    )
    .expect("source should compile with a literal read target");
    let snapshot = catalog_snapshot_with_only_procedure_contract(&ir, 0x6500);

    let error = bind_executable_procedure_plan(&ir, &snapshot)
        .expect_err("read source must resolve as a catalog object, not a runtime input");

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(
        error
            .message()
            .contains("SRPL body references an unbound table/object name"),
        "unexpected dynamic table-name rejection: {}",
        error.message()
    );
}

#[test]
fn executable_plan_rejects_shape_shifting_return_values() {
    let ir = compile_narrow_procedure_signature(
        "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) begin ensure Inventory.ProductStock Stock where ProductId = Stock.ProductId and Stock.AvailableQuantity >= Quantity else fail InsufficientStock; update Inventory.ProductStock set AvailableQuantity = Stock.AvailableQuantity - Quantity where ProductId = Stock.ProductId affected rows 1; return Reservation (Reserved, Extra); end;",
    )
    .expect("source should compile before executable result-shape binding");
    let snapshot = inventory_catalog_snapshot();

    let error = bind_executable_procedure_plan(&ir, &snapshot)
        .expect_err("emit values must not change the declared ResultStream shape");

    assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
    assert!(
        error
            .message()
            .contains("SRPL emit values must match result columns exactly and in order"),
        "unexpected shape-shifting return rejection: {}",
        error.message()
    );
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
    let stock_name = QualifiedName::parse("Inventory.ProductStock").unwrap();
    let reservation_name = QualifiedName::parse("Inventory.Reservation").unwrap();
    let stock_evidence = first
        .evidence
        .find_bound_object(&stock_name)
        .expect("inventory ProductStock table is bound by SRPL evidence");
    assert_eq!(
        stock_evidence.object.name.as_catalog_path(),
        "Inventory.ProductStock"
    );
    assert!(!stock_evidence.shape_hash.is_zero());
    let reservation_evidence = first
        .evidence
        .find_bound_object(&reservation_name)
        .expect("inventory Reservation structured object is bound by SRPL evidence");
    assert_eq!(
        reservation_evidence.object.name.as_catalog_path(),
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
        andromeda_srpl_ir::BoundSrplOperationPlan::UpdateTable {
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
fn executable_plan_rejects_assert_referencing_unbound_read_binding() {
    let mut ir = compile_narrow_procedure_signature(
        "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool);",
    )
    .unwrap();
    ir.body = inventory_reserve_stock_body_ir().unwrap();
    // Move Assert before Read so the assertion's binding reference is unbound.
    ir.body.operations.swap(0, 1);
    ir.body.operations[0].ordinal = 0;
    ir.body.operations[1].ordinal = 1;
    let snapshot = inventory_catalog_snapshot();

    let error = bind_executable_procedure_plan(&ir, &snapshot).unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
    assert!(error.message().contains("unbound read binding"));
}

#[test]
fn binder_supports_a_distinct_read_only_procedure_shape() {
    use andromeda_catalog::{inventory_product_stock_table, inventory_protocol_layout_ref};
    use andromeda_procedure_contract::{ProcedureContractCandidate, ResultStreamContract};
    use andromeda_srpl_ir::{
        SrplBusinessOperationIr, SrplEmitValueIr, SrplPredicateIr, SrplProcedureBodyIr,
        SrplResultStreamIr,
    };

    let next_version = CatalogVersion::new(1);

    // Distinct procedure: read-only multi-row query of ProductStock, no
    // structured-object binding, Many cardinality.
    let candidate = ProcedureContractCandidate {
        object: CatalogObjectRef {
            object_id: CatalogObjectId::new(0x6001),
            name: QualifiedName::parse("Inventory.QueryProductStock").unwrap(),
            kind: ObjectKind::Procedure,
            catalog_version: next_version,
        },
        procedure_id: ProcedureId::new(0x6001),
        stats_version: StatsVersion::new(1),
        protocol_layout: inventory_protocol_layout_ref(),
        inputs: vec![ColumnDescriptor {
            name: "ProductId".to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal: 0,
        }],
        structured_inputs: Vec::new(),
        result_streams: vec![ResultStreamContract {
            stream_id: 1,
            name: "Snapshot".to_string(),
            columns: vec![ColumnDescriptor {
                name: "AvailableQuantity".to_string(),
                data_type: TypeDescriptor::required(ScalarType::I64),
                ordinal: 0,
            }],
            cardinality: ResultStreamCardinality::Many,
            row_count_exact_required: false,
        }],
        required_permissions: vec!["Inventory.QueryProductStock.Execute".to_string()],
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadOnly,
            isolation: IsolationPolicy::Serializable,
            retryable: true,
        },
        compatibility_policy: CompatibilityPolicy::ExactHash,
        result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
        error_policy: ProcedureErrorPolicy {
            rollback_on_error: false,
            allowed_error_codes: Vec::new(),
        },
        multi_result_policy: MultiResultPolicy::SingleResultOnly,
    };
    let contract = candidate.materialize().unwrap();

    let batch = DefinitionBatch {
        batch_id: DefinitionBatchId::new(0x6001),
        database_id: INVENTORY_DATABASE_ID,
        namespace_id: INVENTORY_NAMESPACE_ID,
        base_version: CatalogVersion::new(0),
        operations: vec![
            DefinitionOperation::Create(CatalogDefinition::Table(
                inventory_product_stock_table(next_version).unwrap(),
            )),
            DefinitionOperation::Create(CatalogDefinition::Procedure(contract)),
        ],
    };
    let plan = batch.dry_run().unwrap();
    let mut snapshot = CatalogSnapshot::empty(
        INVENTORY_DATABASE_ID,
        INVENTORY_NAMESPACE_ID,
        batch.base_version,
    );
    snapshot.apply_mutation_plan(&plan.mutation_plan).unwrap();

    let ir = SrplProcedureIr {
        name: QualifiedName::parse("Inventory.QueryProductStock").unwrap(),
        inputs: vec![ColumnDescriptor {
            name: "ProductId".to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal: 0,
        }],
        result_streams: vec![SrplResultStreamIr {
            name: "Snapshot".to_string(),
            cardinality: Cardinality::Many,
            columns: vec![ColumnDescriptor {
                name: "AvailableQuantity".to_string(),
                data_type: TypeDescriptor::required(ScalarType::I64),
                ordinal: 0,
            }],
        }],
        body: SrplProcedureBodyIr {
            operations: vec![
                SrplBusinessOperationIr {
                    ordinal: 0,
                    kind: SrplBusinessOperationKindIr::Read {
                        source: QualifiedName::parse("Inventory.ProductStock").unwrap(),
                        binding: "Stock".to_string(),
                        cardinality: Cardinality::Many,
                        predicates: vec![SrplPredicateIr::InputEqualsField {
                            input: "ProductId".to_string(),
                            binding: "Stock".to_string(),
                            field: "ProductId".to_string(),
                        }],
                    },
                },
                SrplBusinessOperationIr {
                    ordinal: 1,
                    kind: SrplBusinessOperationKindIr::Emit {
                        stream: "Snapshot".to_string(),
                        values: vec![SrplEmitValueIr {
                            column: "AvailableQuantity".to_string(),
                            value: SrplValueIr::Field {
                                binding: "Stock".to_string(),
                                field: "AvailableQuantity".to_string(),
                            },
                        }],
                    },
                },
            ],
        },
    };

    let executable = bind_executable_procedure_plan(&ir, &snapshot).unwrap();
    assert_eq!(executable.body.operations.len(), 2);
    assert_eq!(
        executable.evidence.procedure_object.name.as_catalog_path(),
        "Inventory.QueryProductStock"
    );
    let stock_name = QualifiedName::parse("Inventory.ProductStock").unwrap();
    assert!(executable.evidence.find_bound_object(&stock_name).is_some());
    // No `Inventory.Snapshot` structured object exists; the optional
    // discovery must NOT require one.
    let snapshot_name = QualifiedName::parse("Inventory.Snapshot").unwrap();
    assert!(
        executable
            .evidence
            .find_bound_object(&snapshot_name)
            .is_none()
    );
    assert!(executable.validate().is_ok());

    // Lowering and binding are deterministic across invocations.
    let again = bind_executable_procedure_plan(&ir, &snapshot).unwrap();
    assert_eq!(executable, again);
}

fn catalog_snapshot_with_only_procedure_contract(
    ir: &SrplProcedureIr,
    object_seed: u64,
) -> CatalogSnapshot {
    let catalog_version = CatalogVersion::new(1);
    let mut metadata = contract_metadata();
    metadata.object_id = CatalogObjectId::new(object_seed);
    metadata.procedure_id = ProcedureId::new(object_seed);
    metadata.catalog_version = catalog_version;
    metadata.structured_inputs = Vec::new();

    let definition = lower_ir_to_catalog_definition(ir.clone(), metadata)
        .expect("test IR should materialize as a Procedure contract");
    let batch = DefinitionBatch {
        batch_id: DefinitionBatchId::new(object_seed),
        database_id: INVENTORY_DATABASE_ID,
        namespace_id: INVENTORY_NAMESPACE_ID,
        base_version: CatalogVersion::new(0),
        operations: vec![DefinitionOperation::Create(definition)],
    };
    let plan = batch
        .dry_run()
        .expect("test Procedure-only batch should dry-run");
    let mut snapshot = CatalogSnapshot::empty(
        INVENTORY_DATABASE_ID,
        INVENTORY_NAMESPACE_ID,
        batch.base_version,
    );
    snapshot
        .apply_mutation_plan(&plan.mutation_plan)
        .expect("test Procedure contract should apply to snapshot");
    snapshot
}
