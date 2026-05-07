use andromeda_error::AndromedaErrorKind;
use andromeda_srpl::{
    DiagnosticPhase,
    definition_batch_bridge::{
        SrplDefinitionBatchProcedureSource, SrplProcedureDefinition,
        dry_run_srpl_definition_batch_sources,
    },
};
use andromeda_types::CatalogVersion;

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
