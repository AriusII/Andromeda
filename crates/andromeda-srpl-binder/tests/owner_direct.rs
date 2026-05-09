#![forbid(unsafe_code)]

use andromeda_contract::QualifiedName;
use andromeda_error::AndromedaErrorKind;
use andromeda_srpl_ast::{
    Cardinality, FieldAst, ProcedureAst, ProcedureBodyAst, ResultStreamAst, SourceSpan, Spanned,
};
use andromeda_srpl_binder::{bind_procedure, validate_ast_names_for_diagnostics};
use andromeda_srpl_diagnostics::DiagnosticPhase;
use andromeda_types::{ScalarType, TypeDescriptor};

#[test]
fn owner_direct_bind_procedure_maps_ast_to_signature_without_srpl_facade() {
    let bound = bind_procedure(procedure_ast(
        vec![
            field("ProductId", ScalarType::I64, 0),
            field("Quantity", ScalarType::I64, 1),
        ],
        vec![result_stream(
            "Reservation",
            Cardinality::One,
            vec![field("Reserved", ScalarType::Bool, 0)],
        )],
    ))
    .expect("binder owner must bind AST directly");

    assert_eq!(
        bound.signature.name.as_catalog_path(),
        "Inventory.ReserveStock"
    );
    assert_eq!(bound.signature.accepts[0].name, "ProductId");
    assert_eq!(bound.signature.accepts[1].ordinal, 1);
    assert_eq!(bound.signature.returns[0].name, "Reservation");
    assert_eq!(bound.signature.returns[0].columns[0].name, "Reserved");
    assert!(bound.body.operations.is_empty());
}

#[test]
fn owner_direct_binder_rejects_duplicate_inputs_with_owner_error_kind() {
    let error = bind_procedure(procedure_ast(
        vec![
            field("ProductId", ScalarType::I64, 0),
            field("ProductId", ScalarType::I64, 1),
        ],
        vec![result_stream(
            "Reservation",
            Cardinality::One,
            vec![field("Reserved", ScalarType::Bool, 0)],
        )],
    ))
    .expect_err("binder owner must reject duplicate input names");

    assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
    assert!(error.message().contains("unique"));
}

#[test]
fn owner_direct_binding_diagnostics_preserve_duplicate_result_column_span() {
    let source = "procedure Inventory.ReserveStock accepts () returns Reservation one (Reserved bool, Reserved bool);";
    let mut ast = procedure_ast(
        Vec::new(),
        vec![result_stream(
            "Reservation",
            Cardinality::One,
            vec![
                field("Reserved", ScalarType::Bool, 0),
                field("Reserved", ScalarType::Bool, 1),
            ],
        )],
    );
    ast.results[0].columns[0].name.span = SourceSpan::new(74, 82);
    ast.results[0].columns[1].name.span = SourceSpan::new(89, 97);

    let diagnostic = validate_ast_names_for_diagnostics(&ast, source)
        .expect_err("duplicate result columns must produce source-rich binder diagnostics");

    assert_eq!(diagnostic.phase, DiagnosticPhase::Binding);
    assert_eq!(diagnostic.location, Some(SourceSpan::new(89, 97)));
    assert!(diagnostic.message.contains("Reserved"));
    assert!(diagnostic.message.contains("line 1, column 90"));
}

fn procedure_ast(parameters: Vec<FieldAst>, results: Vec<ResultStreamAst>) -> ProcedureAst {
    ProcedureAst {
        name: Spanned::new(
            QualifiedName::parse("Inventory.ReserveStock").unwrap(),
            SourceSpan::new(0, 22),
        ),
        parameters,
        results,
        body: ProcedureBodyAst {
            operations: Vec::new(),
            span: SourceSpan::new(0, 0),
        },
        span: SourceSpan::new(0, 80),
    }
}

fn result_stream(name: &str, cardinality: Cardinality, columns: Vec<FieldAst>) -> ResultStreamAst {
    ResultStreamAst {
        name: Spanned::new(name.to_string(), SourceSpan::new(0, name.len())),
        cardinality: Spanned::new(cardinality, SourceSpan::new(0, 3)),
        columns,
        span: SourceSpan::new(0, 12),
    }
}

fn field(name: &str, scalar_type: ScalarType, ordinal: u32) -> FieldAst {
    FieldAst {
        name: Spanned::new(name.to_string(), SourceSpan::new(0, name.len())),
        data_type: Spanned::new(
            TypeDescriptor::required(scalar_type),
            SourceSpan::new(0, name.len()),
        ),
        ordinal,
    }
}
