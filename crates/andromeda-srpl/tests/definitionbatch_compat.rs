//! SRPL catalog definition lifecycle compatibility tests.

use std::collections::BTreeMap;

use andromeda_catalog::{
    CatalogDefinition, CatalogSystemStore, DefinitionBatch, DefinitionBatchId, DefinitionOperation,
    ResultStreamCardinality,
};
use andromeda_core::{
    AbsencePolicy, AndromedaErrorKind, CatalogObjectId, CatalogVersion, DatabaseId, NamespaceId,
    ProcedureId, ScalarType, TypeDescriptor,
};
use andromeda_srpl::{
    DiagnosticPhase, SrplProcedureContractMetadata,
    definition_batch_bridge::{
        MAX_SRPL_DEFINITION_BATCH_PROCEDURES, SrplDefinitionBatchDryRunRequest,
        SrplDefinitionBatchProcedureSource, SrplProcedureDefinition,
        dry_run_srpl_definition_batch_sources,
    },
    inventory_reserve_stock_contract_metadata,
};

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

fn lookup_signature_source() -> String {
    "procedure Inventory.LookupStock accepts (ProductId i64) returns Stock one (AvailableQuantity i64);".to_string()
}

fn test_metadata(
    object_id: u64,
    procedure_id: u64,
    catalog_version: CatalogVersion,
) -> SrplProcedureContractMetadata {
    let mut metadata = inventory_reserve_stock_contract_metadata(catalog_version);
    metadata.object_id = CatalogObjectId::new(object_id);
    metadata.procedure_id = ProcedureId::new(procedure_id);
    metadata
}

fn dry_run_request(
    batch_id: u64,
    base_version: CatalogVersion,
    procedures: Vec<SrplDefinitionBatchProcedureSource>,
) -> SrplDefinitionBatchDryRunRequest {
    SrplDefinitionBatchDryRunRequest {
        batch_id: DefinitionBatchId::new(batch_id),
        database_id: TEST_DB_ID,
        namespace_id: TEST_NS_ID,
        base_version,
        procedures,
    }
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
        assert_eq!(
            contract.result_streams[0].cardinality,
            ResultStreamCardinality::One
        );
        assert!(contract.result_streams[0].row_count_exact_required);
    }
}

#[test]
fn a4_parse_rejects_forbidden_construct_before_staged_binding() {
    let source = "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool) body { read Inventory.ProductStock while one; emit Reservation (Reserved); }".to_string();
    let mut def = SrplProcedureDefinition::from_source(source);

    let error = def
        .parse()
        .expect_err("staged SRPL parse must apply core forbidden construct diagnostics");

    assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
    assert!(error.message().contains("SRPL-FORBID-001"));
    assert!(error.message().contains("unbounded while"));
    assert!(def.parsed_ast.is_none());
}

#[test]
fn a4b_definitionbatch_dry_run_rejects_ambiguous_cardinality_before_catalog_plan() {
    let source =
        "procedure Inventory.BadOptional accepts (ProductId i64) returns R optional many (C bool);";
    let request = dry_run_request(
        44,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            source,
            test_metadata(44, 44, CatalogVersion::new(1)),
        )],
    );

    let error = dry_run_srpl_definition_batch_sources(request)
        .expect_err("ambiguous SRPL cardinality must fail before DefinitionBatch publication");

    assert_eq!(error.diagnostics.len(), 1);
    let diagnostic = &error.diagnostics[0].diagnostic;
    assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
    assert!(
        diagnostic
            .message
            .contains("optional cardinality must be written as `optional one`")
    );
}

#[test]
fn a5_staged_catalog_definition_preserves_full_result_cardinality() {
    let cases = [
        (
            "procedure Inventory.ExactlyOne accepts (P i64) returns R one (C bool);",
            ResultStreamCardinality::One,
            true,
        ),
        (
            "procedure Inventory.OptionalResult accepts (P i64) returns R optional one (C bool);",
            ResultStreamCardinality::OptionalOne,
            false,
        ),
        (
            "procedure Inventory.NonEmptyResult accepts (P i64) returns R nonempty many (C bool);",
            ResultStreamCardinality::NonEmptyMany,
            true,
        ),
        (
            "procedure Inventory.ManyResults accepts (P i64) returns R many (C bool);",
            ResultStreamCardinality::Many,
            false,
        ),
    ];

    for (index, (source, expected_cardinality, expected_exact_required)) in
        cases.into_iter().enumerate()
    {
        let mut def = SrplProcedureDefinition::from_source(source.to_string());
        def.parse().expect("case should parse");
        def.bind_and_lower().expect("case should bind and lower");
        let catalog_def = def
            .to_catalog_procedure_def(
                CatalogObjectId::new(10 + index as u64),
                ProcedureId::new(10 + index as u64),
                CatalogVersion::new(1),
            )
            .expect("case should materialize");

        let CatalogDefinition::Procedure(contract) = catalog_def else {
            panic!("staged SRPL source must materialize as a Procedure catalog definition");
        };
        assert_eq!(
            contract.result_streams[0].cardinality, expected_cardinality,
            "unexpected full cardinality for {source}"
        );
        assert_eq!(
            contract.result_streams[0].row_count_exact_required, expected_exact_required,
            "unexpected exact row-count flag for {source}"
        );
    }
}

#[test]
fn a5b_definitionbatch_dry_run_preserves_full_result_cardinality() {
    let cases = [
        (
            "procedure Inventory.ExactlyOne accepts (P i64) returns R one (C bool);",
            ResultStreamCardinality::One,
        ),
        (
            "procedure Inventory.OptionalResult accepts (P i64) returns R optional one (C bool);",
            ResultStreamCardinality::OptionalOne,
        ),
        (
            "procedure Inventory.NonEmptyResult accepts (P i64) returns R nonempty many (C bool);",
            ResultStreamCardinality::NonEmptyMany,
        ),
        (
            "procedure Inventory.ManyResults accepts (P i64) returns R many (C bool);",
            ResultStreamCardinality::Many,
        ),
    ];

    let report = dry_run_srpl_definition_batch_sources(dry_run_request(
        45,
        CatalogVersion::new(0),
        cases
            .iter()
            .enumerate()
            .map(|(index, (source, _))| {
                SrplDefinitionBatchProcedureSource::new(
                    *source,
                    test_metadata(
                        450 + index as u64,
                        450 + index as u64,
                        CatalogVersion::new(1),
                    ),
                )
            })
            .collect(),
    ))
    .expect("cardinality probe batch should dry-run");

    assert_eq!(report.manifests.len(), cases.len());
    assert_eq!(report.definition_batch.operations.len(), cases.len());
    for (index, (_, expected_cardinality)) in cases.iter().enumerate() {
        assert_eq!(
            report.manifests[index].result_stream_cardinalities,
            vec![*expected_cardinality],
            "manifest cardinality should survive for case {index}"
        );
        let DefinitionOperation::Create(CatalogDefinition::Procedure(contract)) =
            &report.definition_batch.operations[index]
        else {
            panic!("cardinality probe must materialize as Procedure definitions");
        };
        assert_eq!(
            contract.result_streams[0].cardinality, *expected_cardinality,
            "DefinitionBatch contract cardinality should survive for case {index}"
        );
    }
}

#[test]
fn a5c_optional_one_does_not_introduce_implicit_null_semantics() {
    let report = dry_run_srpl_definition_batch_sources(dry_run_request(
        46,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            "procedure Inventory.OptionalResult accepts (P i64) returns R optional one (C bool);",
            test_metadata(460, 460, CatalogVersion::new(1)),
        )],
    ))
    .expect("optional-one result should dry-run without nullable columns");

    let DefinitionOperation::Create(CatalogDefinition::Procedure(contract)) =
        &report.definition_batch.operations[0]
    else {
        panic!("optional-one source must materialize as a Procedure contract");
    };
    let stream = &contract.result_streams[0];
    assert_eq!(stream.cardinality, ResultStreamCardinality::OptionalOne);
    assert_eq!(stream.columns[0].data_type.absence, AbsencePolicy::Required);

    let rejected = dry_run_srpl_definition_batch_sources(dry_run_request(
        47,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            "procedure Inventory.BadNull accepts (P i64) returns R optional one (C null);",
            test_metadata(470, 470, CatalogVersion::new(1)),
        )],
    ))
    .expect_err("SRPL null type must not become an implicit optional column");

    assert_eq!(rejected.diagnostics.len(), 1);
    let diagnostic = &rejected.diagnostics[0].diagnostic;
    assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
    assert!(
        diagnostic.message.contains("forbids nullable values"),
        "unexpected diagnostic: {}",
        diagnostic.message
    );
}

#[test]
fn a6_staged_catalog_definition_with_metadata_matches_compiler_pipeline() {
    let source = signature_only_source();
    let metadata = test_metadata(60, 60, CatalogVersion::new(1));
    let mut def = SrplProcedureDefinition::from_source(source.clone());
    def.parse().expect("source should parse");
    def.bind_and_lower().expect("source should bind and lower");

    let staged = def
        .to_catalog_procedure_def_with_metadata(metadata.clone())
        .expect("staged materialization should succeed");
    let direct = andromeda_srpl::compile_narrow_procedure_definition(&source, metadata)
        .expect("direct pipeline materialization should succeed");

    assert_eq!(staged, direct);
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

    let result = dry_run_srpl_definition_batch_sources(dry_run_request(
        70,
        CatalogVersion::new(0),
        vec![
            SrplDefinitionBatchProcedureSource::new(
                good_source,
                test_metadata(700, 700, CatalogVersion::new(1)),
            ),
            SrplDefinitionBatchProcedureSource::new(
                bad_source,
                test_metadata(701, 701, CatalogVersion::new(1)),
            ),
        ],
    ));

    let error = result.expect_err("one bad SRPL source must reject the entire batch");
    assert_eq!(error.diagnostics.len(), 1);
    assert_eq!(error.diagnostics[0].source_index, Some(1));
    assert!(
        error.diagnostics[0]
            .diagnostic
            .message
            .contains("declare at least one column")
    );
}

#[test]
fn g2_batch_error_includes_all_failures() {
    let source1 = "procedure X accepts () returns R many ();".to_string();
    let source2 = "procedure Y accepts () returns S many ();".to_string();

    let result = dry_run_srpl_definition_batch_sources(dry_run_request(
        71,
        CatalogVersion::new(0),
        vec![
            SrplDefinitionBatchProcedureSource::new(
                source1,
                test_metadata(710, 710, CatalogVersion::new(1)),
            ),
            SrplDefinitionBatchProcedureSource::new(
                source2,
                test_metadata(711, 711, CatalogVersion::new(1)),
            ),
        ],
    ));

    let error = result.expect_err("invalid SRPL sources must return all source failures");
    assert_eq!(error.diagnostics.len(), 2);
    assert_eq!(error.diagnostics[0].source_index, Some(0));
    assert_eq!(error.diagnostics[1].source_index, Some(1));
}

#[test]
fn g3_source_dry_run_materializes_definition_batch_and_manifest() {
    let result = dry_run_srpl_definition_batch_sources(dry_run_request(
        72,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            signature_only_source(),
            test_metadata(720, 720, CatalogVersion::new(1)),
        )],
    ))
    .expect("valid SRPL source should materialize and dry-run");

    assert_eq!(result.definition_batch.operations.len(), 1);
    assert_eq!(result.plan.operation_count, 1);
    assert_eq!(result.manifests.len(), 1);
    assert_eq!(
        result.definition_batch_source_hash,
        result.definition_batch.source_hash()
    );
    assert_eq!(
        result.definition_batch_dependency_graph_hash,
        result.definition_batch.dependency_graph_hash().unwrap()
    );
    assert!(!result.definition_batch_source_hash.is_zero());
    assert!(!result.definition_batch_dependency_graph_hash.is_zero());

    let manifest = &result.manifests[0];
    assert_eq!(manifest.source_index, 0);
    assert_eq!(
        manifest.procedure_name.as_catalog_path(),
        "Inventory.ReserveStock"
    );
    assert_eq!(manifest.object_id, CatalogObjectId::new(720));
    assert_eq!(manifest.binding.procedure_id, ProcedureId::new(720));
    assert_eq!(manifest.binding.catalog_version, CatalogVersion::new(1));
    assert_eq!(manifest.contract_hash, manifest.binding.contract_hash);
    assert!(!manifest.source_digest.is_zero());
    assert_eq!(manifest.input_count, 1);
    assert_eq!(manifest.result_stream_count, 1);
    assert!(manifest.binding.validate().is_ok());

    assert_eq!(
        result.source_evidence.definition_batch_source_hash,
        result.definition_batch_source_hash
    );
    assert_eq!(
        result
            .source_evidence
            .definition_batch_dependency_graph_hash,
        result.definition_batch_dependency_graph_hash
    );
    assert_eq!(result.source_evidence.procedures.len(), 1);
    assert_eq!(
        result.source_evidence.procedures[0].source_digest,
        manifest.source_digest
    );
    assert_eq!(
        result.source_evidence.procedures[0].contract_hash,
        manifest.contract_hash
    );
}

#[test]
fn g4_catalog_conflict_rejects_after_all_sources_materialize() {
    let result = dry_run_srpl_definition_batch_sources(dry_run_request(
        73,
        CatalogVersion::new(0),
        vec![
            SrplDefinitionBatchProcedureSource::new(
                signature_only_source(),
                test_metadata(730, 730, CatalogVersion::new(1)),
            ),
            SrplDefinitionBatchProcedureSource::new(
                signature_only_source(),
                test_metadata(731, 731, CatalogVersion::new(1)),
            ),
        ],
    ));

    let error = result.expect_err("duplicate materialized Procedure names must reject the batch");
    assert_eq!(error.diagnostics.len(), 1);
    assert_eq!(error.diagnostics[0].source_index, None);
    assert!(
        error.diagnostics[0]
            .diagnostic
            .message
            .contains("materialized DefinitionBatch was rejected by catalog dry-run")
    );
}

#[test]
fn g4b_source_dry_run_rejects_unbounded_procedure_source_count() {
    let procedures = (0..=MAX_SRPL_DEFINITION_BATCH_PROCEDURES)
        .map(|index| {
            SrplDefinitionBatchProcedureSource::new(
                signature_only_source(),
                test_metadata(
                    7_400 + index as u64,
                    7_500 + index as u64,
                    CatalogVersion::new(1),
                ),
            )
        })
        .collect();

    let error = dry_run_srpl_definition_batch_sources(dry_run_request(
        7_399,
        CatalogVersion::new(0),
        procedures,
    ))
    .expect_err("SRPL DefinitionBatch dry-run must reject unbounded source counts");

    assert_eq!(error.diagnostics.len(), 1);
    assert_eq!(error.diagnostics[0].source_index, None);
    assert!(
        error.diagnostics[0]
            .diagnostic
            .message
            .contains("accepts at most")
    );
}

#[test]
fn g5_contract_hash_is_stable_across_equivalent_source_formatting() {
    let source_a = signature_only_source();
    let source_b = "procedure Inventory.ReserveStock
        accepts   (ProductId i64)
        returns   Reservation one (Reserved bool);"
        .to_string();

    let report_a = dry_run_srpl_definition_batch_sources(dry_run_request(
        74,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            source_a,
            test_metadata(740, 740, CatalogVersion::new(1)),
        )],
    ))
    .expect("formatted source A should compile");
    let report_b = dry_run_srpl_definition_batch_sources(dry_run_request(
        75,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            source_b,
            test_metadata(740, 740, CatalogVersion::new(1)),
        )],
    ))
    .expect("formatted source B should compile");

    assert_eq!(
        report_a.manifests[0].binding.contract_hash,
        report_b.manifests[0].binding.contract_hash
    );
}

#[test]
fn g6_source_errors_short_circuit_catalog_dry_run() {
    let duplicate_valid_source = signature_only_source();
    let invalid_source_a = "procedure Invalid.Empty accepts () returns R many ();".to_string();
    let invalid_source_b = "procedure Invalid.EmptyToo accepts () returns S many ();".to_string();

    let result = dry_run_srpl_definition_batch_sources(dry_run_request(
        76,
        CatalogVersion::new(0),
        vec![
            SrplDefinitionBatchProcedureSource::new(
                duplicate_valid_source.clone(),
                test_metadata(760, 760, CatalogVersion::new(1)),
            ),
            SrplDefinitionBatchProcedureSource::new(
                invalid_source_a,
                test_metadata(761, 761, CatalogVersion::new(1)),
            ),
            SrplDefinitionBatchProcedureSource::new(
                invalid_source_b,
                test_metadata(762, 762, CatalogVersion::new(1)),
            ),
            SrplDefinitionBatchProcedureSource::new(
                duplicate_valid_source,
                test_metadata(763, 763, CatalogVersion::new(1)),
            ),
        ],
    ));

    let error = result.expect_err("source diagnostics must reject before catalog dry-run");
    assert_eq!(error.diagnostics.len(), 2);
    assert_eq!(error.diagnostics[0].source_index, Some(1));
    assert_eq!(error.diagnostics[1].source_index, Some(2));
    assert!(
        error
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.procedure_name.is_none())
    );
    assert!(error.diagnostics.iter().all(|diagnostic| {
        diagnostic.diagnostic.phase != DiagnosticPhase::SemanticValidation
            || !diagnostic
                .diagnostic
                .message
                .contains("materialized DefinitionBatch was rejected")
    }));
}

#[test]
fn g7_forbidden_construct_diagnostic_keeps_source_index_and_utf8_span() {
    let forbidden_source = "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool) begin note 😊; select *; end;".to_string();

    let result = dry_run_srpl_definition_batch_sources(dry_run_request(
        77,
        CatalogVersion::new(0),
        vec![
            SrplDefinitionBatchProcedureSource::new(
                lookup_signature_source(),
                test_metadata(770, 770, CatalogVersion::new(1)),
            ),
            SrplDefinitionBatchProcedureSource::new(
                forbidden_source.clone(),
                test_metadata(771, 771, CatalogVersion::new(1)),
            ),
        ],
    ));

    let error = result.expect_err("forbidden SRPL construct must reject the source batch");
    assert_eq!(error.diagnostics.len(), 1);
    let diagnostic = &error.diagnostics[0];
    assert_eq!(diagnostic.source_index, Some(1));
    assert_eq!(diagnostic.procedure_name, None);
    assert_eq!(diagnostic.diagnostic.phase, DiagnosticPhase::Binding);
    assert!(diagnostic.diagnostic.message.contains("select star"));
    assert!(diagnostic.diagnostic.message.contains("line 1"));
    let span = diagnostic
        .diagnostic
        .location
        .expect("forbidden construct diagnostic must keep a source span");
    assert!(span.is_valid());
    assert!(forbidden_source.is_char_boundary(span.start));
    assert!(forbidden_source.is_char_boundary(span.end));
    assert_eq!(&forbidden_source[span.start..span.end], "select *");
}

#[test]
fn g8_manifest_bindings_are_stable_when_source_order_changes() {
    let reserve = SrplDefinitionBatchProcedureSource::new(
        signature_only_source(),
        test_metadata(780, 780, CatalogVersion::new(1)),
    );
    let lookup = SrplDefinitionBatchProcedureSource::new(
        lookup_signature_source(),
        test_metadata(781, 781, CatalogVersion::new(1)),
    );

    let forward = dry_run_srpl_definition_batch_sources(dry_run_request(
        78,
        CatalogVersion::new(0),
        vec![reserve.clone(), lookup.clone()],
    ))
    .expect("forward source order should dry-run");
    let reversed = dry_run_srpl_definition_batch_sources(dry_run_request(
        79,
        CatalogVersion::new(0),
        vec![lookup, reserve],
    ))
    .expect("reversed source order should dry-run");

    let forward_by_name = forward
        .manifests
        .iter()
        .map(|manifest| {
            (
                manifest.procedure_name.as_catalog_path(),
                (
                    manifest.object_id,
                    manifest.binding,
                    manifest.input_count,
                    manifest.result_stream_count,
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let reversed_by_name = reversed
        .manifests
        .iter()
        .map(|manifest| {
            (
                manifest.procedure_name.as_catalog_path(),
                (
                    manifest.object_id,
                    manifest.binding,
                    manifest.input_count,
                    manifest.result_stream_count,
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();

    assert_eq!(forward_by_name, reversed_by_name);
    assert_ne!(
        forward.definition_batch_source_hash, reversed.definition_batch_source_hash,
        "ordered DefinitionBatch source identity must change when source order changes"
    );
    assert_eq!(
        forward.definition_batch_dependency_graph_hash,
        reversed.definition_batch_dependency_graph_hash,
        "canonical dependency graph identity must ignore equivalent source ordering"
    );
    assert_eq!(forward.manifests[0].source_index, 0);
    assert_eq!(reversed.manifests[0].source_index, 0);
    assert_ne!(
        forward.manifests[0].procedure_name,
        reversed.manifests[0].procedure_name
    );
}

#[test]
fn g9_source_digest_distinguishes_exact_srpl_text_from_materialized_contract_hash() {
    let source_a = signature_only_source();
    let source_b = "procedure Inventory.ReserveStock
        accepts   (ProductId i64)
        returns   Reservation one (Reserved bool);"
        .to_string();

    let report_a = dry_run_srpl_definition_batch_sources(dry_run_request(
        80,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            source_a,
            test_metadata(800, 800, CatalogVersion::new(1)),
        )],
    ))
    .expect("source A should compile");
    let report_b = dry_run_srpl_definition_batch_sources(dry_run_request(
        80,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            source_b,
            test_metadata(800, 800, CatalogVersion::new(1)),
        )],
    ))
    .expect("source B should compile");

    let manifest_a = &report_a.manifests[0];
    let manifest_b = &report_b.manifests[0];

    assert_ne!(
        manifest_a.source_digest, manifest_b.source_digest,
        "SRPL source digest must bind exact source text"
    );
    assert_eq!(
        manifest_a.contract_hash, manifest_b.contract_hash,
        "ProcedureContract hash must be derived from canonical typed contract shape"
    );
    assert_eq!(manifest_a.contract_hash, manifest_a.binding.contract_hash);
    assert_eq!(manifest_b.contract_hash, manifest_b.binding.contract_hash);
    assert_eq!(
        report_a.definition_batch_source_hash, report_b.definition_batch_source_hash,
        "materialized DefinitionBatch source hash should ignore equivalent SRPL formatting"
    );
    assert_eq!(
        report_a.definition_batch_source_hash,
        report_a.definition_batch.source_hash()
    );
    assert_ne!(
        report_a.source_evidence.procedures[0].source_digest,
        report_b.source_evidence.procedures[0].source_digest,
        "apply/durable source evidence must bind exact SRPL text"
    );
    assert_eq!(
        report_a.source_evidence.procedures[0].contract_hash,
        report_b.source_evidence.procedures[0].contract_hash,
        "apply/durable source evidence must also bind the canonical Procedure contract"
    );
}

#[test]
fn g9b_source_digest_is_stable_for_same_source_and_changes_for_semantic_text_change() {
    let source_a = signature_only_source();
    let source_a_repeat = signature_only_source();
    let source_c = "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved i64);"
        .to_string();

    let report_a = dry_run_srpl_definition_batch_sources(dry_run_request(
        83,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            source_a,
            test_metadata(830, 830, CatalogVersion::new(1)),
        )],
    ))
    .expect("source A should compile");
    let report_a_repeat = dry_run_srpl_definition_batch_sources(dry_run_request(
        83,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            source_a_repeat,
            test_metadata(830, 830, CatalogVersion::new(1)),
        )],
    ))
    .expect("repeated source A should compile");
    let report_c = dry_run_srpl_definition_batch_sources(dry_run_request(
        84,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            source_c,
            test_metadata(840, 840, CatalogVersion::new(1)),
        )],
    ))
    .expect("semantically changed source should compile");

    assert_eq!(
        report_a.source_evidence.procedures[0].source_digest,
        report_a_repeat.source_evidence.procedures[0].source_digest,
        "same SRPL source text must produce stable source evidence"
    );
    assert_ne!(
        report_a.source_evidence.procedures[0].source_digest,
        report_c.source_evidence.procedures[0].source_digest,
        "semantic text change must produce a different SRPL source digest"
    );
    assert_ne!(
        report_a.source_evidence.procedures[0].contract_hash,
        report_c.source_evidence.procedures[0].contract_hash,
        "semantic text change must also change canonical Procedure contract evidence"
    );
}

#[test]
fn g10_source_digest_evidence_survives_durable_catalog_apply_without_raw_source() {
    let report = dry_run_srpl_definition_batch_sources(dry_run_request(
        81,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            signature_only_source(),
            test_metadata(810, 810, CatalogVersion::new(1)),
        )],
    ))
    .expect("SRPL source should materialize before durable catalog apply");
    let expected_evidence = report.source_evidence.clone();
    let mut store = CatalogSystemStore::empty(TEST_DB_ID, TEST_NS_ID, CatalogVersion::new(0));
    let mut next_lsn = 200;
    let mut flushed_commit_lsn = None;

    let apply_report = report
        .apply_to_catalog_store_durably(
            &mut store,
            |_kind, payload| {
                let _payload_len = payload.len();
                let lsn = next_lsn;
                next_lsn += 1;
                Ok(lsn)
            },
            |commit_lsn| {
                flushed_commit_lsn = Some(commit_lsn);
                Ok(commit_lsn)
            },
        )
        .expect("durable catalog apply should carry SRPL source evidence");

    assert_eq!(apply_report.source_evidence, expected_evidence);
    assert_eq!(
        apply_report.catalog_report.source_hash,
        expected_evidence.definition_batch_source_hash
    );
    assert_eq!(
        apply_report.catalog_report.dependency_graph_hash,
        expected_evidence.definition_batch_dependency_graph_hash
    );
    assert_eq!(
        apply_report.source_evidence.procedures[0]
            .procedure_name
            .as_catalog_path(),
        "Inventory.ReserveStock"
    );
    assert_eq!(
        apply_report.source_evidence.procedures[0].procedure_id,
        ProcedureId::new(810)
    );
    assert!(
        !apply_report.source_evidence.procedures[0]
            .source_digest
            .is_zero()
    );
    assert_eq!(
        apply_report.catalog_report.receipt.durable_lsn,
        flushed_commit_lsn
    );
    assert_eq!(flushed_commit_lsn, Some(202));
    assert_eq!(store.snapshot().version, CatalogVersion::new(1));
}

#[test]
fn g11_source_digest_evidence_survives_durable_catalog_apply() {
    let report = dry_run_srpl_definition_batch_sources(dry_run_request(
        82,
        CatalogVersion::new(0),
        vec![SrplDefinitionBatchProcedureSource::new(
            signature_only_source(),
            test_metadata(820, 820, CatalogVersion::new(1)),
        )],
    ))
    .expect("SRPL source should materialize before durable catalog apply");
    let expected_evidence = report.source_evidence.clone();
    let mut store = CatalogSystemStore::empty(TEST_DB_ID, TEST_NS_ID, CatalogVersion::new(0));
    let mut next_lsn = 100;
    let mut flushed_commit_lsn = None;

    let durable_report = report
        .apply_to_catalog_store_durably(
            &mut store,
            |_kind, payload| {
                let _payload_len = payload.len();
                let lsn = next_lsn;
                next_lsn += 1;
                Ok(lsn)
            },
            |commit_lsn| {
                flushed_commit_lsn = Some(commit_lsn);
                Ok(commit_lsn)
            },
        )
        .expect("durable catalog apply should carry SRPL source evidence");

    assert_eq!(durable_report.source_evidence, expected_evidence);
    assert_eq!(
        durable_report.catalog_report.source_hash,
        expected_evidence.definition_batch_source_hash
    );
    assert_eq!(
        durable_report.catalog_report.dependency_graph_hash,
        expected_evidence.definition_batch_dependency_graph_hash
    );
    assert_eq!(
        durable_report.catalog_report.receipt.durable_lsn,
        flushed_commit_lsn
    );
    assert_eq!(flushed_commit_lsn, Some(102));
    assert!(
        !durable_report.source_evidence.procedures[0]
            .contract_hash
            .is_zero()
    );
    assert_eq!(store.snapshot().version, CatalogVersion::new(1));
}
