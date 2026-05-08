use andromeda_catalog::{CatalogObjectRef, ObjectKind, ProcedureContractRef, QualifiedName};
use andromeda_error::{AndromedaError, AndromedaErrorKind};
use andromeda_srpl::{
    Cardinality, DiagnosticPhase, SrplAssignmentIr, SrplEmitValueIr, SrplPredicateIr, SrplValueIr,
    definition_batch_bridge::{
        SrplDefinitionBatchProcedureSource, SrplProcedureDefinition,
        dry_run_srpl_definition_batch_sources,
    },
    execution_adapter::{
        SrplAssertRequest, SrplBoundValue, SrplEmitRequest, SrplOperationContext, SrplReadRequest,
        SrplRowBound, SrplUpdateRequest,
    },
};
use andromeda_types::{CatalogObjectId, CatalogVersion, ContractHash, ProcedureId};

use crate::support::{
    dry_run_request, lookup_signature_source, signature_only_source, test_metadata,
};

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
fn a4b_parse_staged_preserves_structured_diagnostic() {
    let source = "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool) begin note; select *; end;".to_string();
    let mut def = SrplProcedureDefinition::from_source(source.clone());

    let diagnostic = def
        .parse_staged()
        .expect_err("structured staged parse diagnostic must be returned");

    assert_eq!(diagnostic.phase, DiagnosticPhase::Binding);
    assert!(diagnostic.message.contains("select star"));
    let span = diagnostic
        .location
        .expect("staged parse diagnostic must keep the source span");
    assert!(span.is_valid());
    assert_eq!(&source[span.start..span.end], "select *");
    assert!(def.parsed_ast.is_none());
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
fn g8_definitionbatch_collects_stable_srpl_doctrine_diagnostics() {
    struct Case {
        source: &'static str,
        phase: DiagnosticPhase,
        expected_message: &'static str,
        expected_span: &'static str,
    }

    let cases = [
        Case {
            source: "procedure Inventory.DynamicTableProbe accepts (TableName text(64)) returns R many (C bool); dynamic SQL",
            phase: DiagnosticPhase::Binding,
            expected_message: "SRPL-FORBID-006",
            expected_span: "dynamic SQL",
        },
        Case {
            source: "procedure Inventory.DynamicPredicateProbe accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) begin ensure Inventory.ProductStock Stock where ProductId = Stock.ProductId or Stock.AvailableQuantity >= Quantity else fail InsufficientStock; return Reservation (Reserved); end;",
            phase: DiagnosticPhase::Parsing,
            expected_message: "expected contextual SRPL keyword and",
            expected_span: "or",
        },
        Case {
            source: "procedure Inventory.ShapeShiftProbe accepts () returns R one (C bool), S one (D bool);",
            phase: DiagnosticPhase::Parsing,
            expected_message: "only one narrow procedure declaration is accepted",
            expected_span: ",",
        },
        Case {
            source: "procedure Inventory.AmbiguousCardinalityProbe accepts () returns R optional many (C bool);",
            phase: DiagnosticPhase::Parsing,
            expected_message: "SRPL optional cardinality must be written as `optional one`",
            expected_span: "optional",
        },
        Case {
            source: "procedure Inventory.NullProbe accepts () returns R optional one (C null);",
            phase: DiagnosticPhase::Parsing,
            expected_message: "SRPL V0 forbids nullable values",
            expected_span: "null",
        },
    ];

    let result = dry_run_srpl_definition_batch_sources(dry_run_request(
        78,
        CatalogVersion::new(0),
        cases
            .iter()
            .enumerate()
            .map(|(index, case)| {
                let id = 780 + index as u64;
                SrplDefinitionBatchProcedureSource::new(
                    case.source,
                    test_metadata(id, id, CatalogVersion::new(1)),
                )
            })
            .collect(),
    ));

    let error = result.expect_err("SRPL doctrine diagnostics must reject the batch");
    assert_eq!(error.diagnostics.len(), cases.len());
    for (index, case) in cases.iter().enumerate() {
        let source_diagnostic = &error.diagnostics[index];
        assert_eq!(source_diagnostic.source_index, Some(index));
        assert_eq!(source_diagnostic.procedure_name, None);
        assert_eq!(source_diagnostic.diagnostic.phase, case.phase);
        assert!(
            source_diagnostic
                .diagnostic
                .message
                .contains(case.expected_message),
            "unexpected diagnostic for source[{index}]: {}",
            source_diagnostic.diagnostic.message
        );
        assert!(
            source_diagnostic
                .diagnostic
                .message
                .contains("line 1, column"),
            "source diagnostic must include stable line/column context: {}",
            source_diagnostic.diagnostic.message
        );
        assert_srpl_source_span(
            case.source,
            &source_diagnostic.diagnostic,
            case.expected_span,
        );
    }
}

#[test]
fn adapter_public_requests_reject_sql_like_symbols_when_built_directly() {
    let read_error = SrplReadRequest::new(
        adapter_context(0),
        adapter_table_ref(),
        Cardinality::Many,
        SrplRowBound::at_most(1).expect("test bound should be valid"),
        vec![SrplPredicateIr::InputEqualsField {
            input: "select".to_string(),
            binding: "stock".to_string(),
            field: "ProductId".to_string(),
        }],
    )
    .expect_err("direct read request must reject SQL-like predicate symbols");
    assert_sql_like_rejection(&read_error);

    let update_error = SrplUpdateRequest::new(
        adapter_context(1),
        adapter_table_ref(),
        SrplRowBound::exact(1).expect("test bound should be valid"),
        Vec::new(),
        vec![SrplAssignmentIr {
            field: "Reserved".to_string(),
            value: SrplValueIr::Input("where".to_string()),
        }],
    )
    .expect_err("direct update request must reject SQL-like value symbols");
    assert_sql_like_rejection(&update_error);

    let emit_error = SrplEmitRequest::new(
        adapter_context(2),
        "Reservation",
        Cardinality::One,
        SrplRowBound::exact(1).expect("test bound should be valid"),
        vec![SrplEmitValueIr {
            column: "from".to_string(),
            value: SrplValueIr::bool(true),
        }],
    )
    .expect_err("direct emit request must reject SQL-like emit columns");
    assert_sql_like_rejection(&emit_error);

    let assert_error = SrplAssertRequest::new(adapter_context(3), good_adapter_predicate(), "sql")
        .expect_err("direct assert request must reject SQL-like failure codes");
    assert_sql_like_rejection(&assert_error);
}

#[test]
fn adapter_bound_value_rejects_implicit_null_for_required_boundary() {
    assert!(SrplBoundValue::Null.is_null());

    let error = SrplBoundValue::Null
        .reject_implicit_null("SRPL adapter input")
        .expect_err("required SRPL adapter values must not accept implicit Null");

    assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
    assert!(error.message().contains("implicit Null"));
    assert_eq!(
        SrplBoundValue::Bool(true)
            .reject_implicit_null("SRPL adapter input")
            .expect("non-null adapter values should pass"),
        SrplBoundValue::Bool(true)
    );
}

fn adapter_context(ordinal: u32) -> SrplOperationContext {
    SrplOperationContext::new(
        ProcedureContractRef {
            procedure_id: ProcedureId::new(9_001),
            contract_hash: ContractHash::test_vector(0xC1),
            catalog_version: CatalogVersion::new(1),
        },
        ordinal,
    )
    .expect("test operation context should be valid")
}

fn adapter_table_ref() -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(9_002),
        name: QualifiedName::parse("Inventory.Stock").expect("test qualified name should be valid"),
        kind: ObjectKind::Table,
        catalog_version: CatalogVersion::new(1),
    }
}

fn good_adapter_predicate() -> SrplPredicateIr {
    SrplPredicateIr::FieldGreaterThanOrEqualInput {
        binding: "stock".to_string(),
        field: "Available".to_string(),
        input: "Quantity".to_string(),
    }
}

fn assert_sql_like_rejection(error: &AndromedaError) {
    assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
    assert!(
        error.message().contains("SQL-like"),
        "unexpected adapter rejection: {}",
        error.message()
    );
}

fn assert_srpl_source_span(
    source: &str,
    diagnostic: &andromeda_srpl::SrplDiagnostic,
    expected_span: &str,
) {
    let span = diagnostic
        .location
        .expect("diagnostic must retain a source span");
    assert!(span.is_valid(), "invalid diagnostic span: {span:?}");
    assert!(source.is_char_boundary(span.start));
    assert!(source.is_char_boundary(span.end));
    assert_eq!(&source[span.start..span.end], expected_span);
}
