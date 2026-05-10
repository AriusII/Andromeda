#![forbid(unsafe_code)]

use andromeda_procedure_contract::QualifiedName;
use andromeda_srpl_ast::{
    BusinessOperationAst, BusinessOperationKindAst, Cardinality, FieldAst, ProcedureAst,
    ProcedureBodyAst, ResultStreamAst, SourceSpan, Spanned,
};
use andromeda_types::{AbsencePolicy, ScalarType, TypeDescriptor};

fn span(start: usize, end: usize) -> SourceSpan {
    SourceSpan::new(start, end)
}

fn qualified(parts: &[&str]) -> QualifiedName {
    QualifiedName::new(parts.iter().copied()).expect("test qualified name must be valid")
}

fn field(name: &str, scalar: ScalarType, ordinal: u32, start: usize, end: usize) -> FieldAst {
    FieldAst {
        name: Spanned::new(name.to_string(), span(start, end)),
        data_type: Spanned::new(TypeDescriptor::required(scalar), span(start, end)),
        ordinal,
    }
}

#[test]
fn ast_owner_preserves_spanned_required_field_shapes() {
    let product_id = field("ProductId", ScalarType::I64, 0, 28, 41);

    assert_eq!(product_id.name.value, "ProductId");
    assert_eq!(product_id.name.span, span(28, 41));
    assert_eq!(product_id.data_type.value.scalar, ScalarType::I64);
    assert_eq!(product_id.data_type.value.absence, AbsencePolicy::Required);
    assert_eq!(product_id.ordinal, 0);
}

#[test]
fn ast_owner_preserves_procedure_result_and_operation_shapes() {
    let procedure = ProcedureAst {
        name: Spanned::new(qualified(&["Inventory", "Lookup"]), span(10, 26)),
        parameters: vec![field("ProductId", ScalarType::I64, 0, 36, 49)],
        results: vec![ResultStreamAst {
            name: Spanned::new("Rows".to_string(), span(58, 62)),
            cardinality: Spanned::new(Cardinality::NonEmptyMany, span(63, 76)),
            columns: vec![field("Reserved", ScalarType::Bool, 0, 78, 91)],
            span: span(58, 92),
        }],
        body: ProcedureBodyAst {
            operations: vec![
                BusinessOperationAst {
                    ordinal: 0,
                    kind: BusinessOperationKindAst::Read {
                        source: Spanned::new(
                            qualified(&["Inventory", "ProductStock"]),
                            span(105, 127),
                        ),
                        binding: Spanned::new("Stock".to_string(), span(128, 133)),
                        cardinality: Spanned::new(Cardinality::OptionalOne, span(134, 146)),
                    },
                    span: span(100, 146),
                },
                BusinessOperationAst {
                    ordinal: 1,
                    kind: BusinessOperationKindAst::Emit {
                        stream: Spanned::new("Rows".to_string(), span(153, 157)),
                        values: vec![Spanned::new("Reserved".to_string(), span(159, 167))],
                    },
                    span: span(148, 168),
                },
            ],
            span: span(95, 170),
        },
        span: span(0, 171),
    };

    assert_eq!(procedure.name.value.as_catalog_path(), "Inventory.Lookup");
    assert_eq!(procedure.parameters.len(), 1);
    assert_eq!(
        procedure.results[0].cardinality.value,
        Cardinality::NonEmptyMany
    );
    assert_eq!(procedure.body.operations.len(), 2);
    assert_eq!(procedure.body.operations[0].ordinal, 0);
    assert_eq!(procedure.body.operations[1].ordinal, 1);

    match &procedure.body.operations[0].kind {
        BusinessOperationKindAst::Read {
            source,
            binding,
            cardinality,
        } => {
            assert_eq!(source.value.as_catalog_path(), "Inventory.ProductStock");
            assert_eq!(binding.value, "Stock");
            assert_eq!(cardinality.value, Cardinality::OptionalOne);
        },
        other => panic!("first AST operation must remain Read, got {other:?}"),
    }

    match &procedure.body.operations[1].kind {
        BusinessOperationKindAst::Emit { stream, values } => {
            assert_eq!(stream.value, "Rows");
            assert_eq!(values[0].value, "Reserved");
        },
        other => panic!("second AST operation must remain Emit, got {other:?}"),
    }
}

#[test]
fn ast_owner_carries_all_cardinality_variants_without_collapsing_them() {
    let streams = [
        (Cardinality::One, "OneRows"),
        (Cardinality::OptionalOne, "MaybeRows"),
        (Cardinality::Many, "Rows"),
        (Cardinality::NonEmptyMany, "RequiredRows"),
    ]
    .into_iter()
    .enumerate()
    .map(|(ordinal, (cardinality, name))| ResultStreamAst {
        name: Spanned::new(name.to_string(), span(ordinal, ordinal + 1)),
        cardinality: Spanned::new(cardinality, span(ordinal + 10, ordinal + 11)),
        columns: vec![field("C", ScalarType::Bool, 0, ordinal + 20, ordinal + 21)],
        span: span(ordinal, ordinal + 21),
    })
    .collect::<Vec<_>>();

    assert_eq!(streams[0].cardinality.value, Cardinality::One);
    assert_eq!(streams[1].cardinality.value, Cardinality::OptionalOne);
    assert_eq!(streams[2].cardinality.value, Cardinality::Many);
    assert_eq!(streams[3].cardinality.value, Cardinality::NonEmptyMany);
    assert!(streams[0].cardinality.value.requires_exact_row_count());
    assert!(!streams[1].cardinality.value.requires_exact_row_count());
    assert!(!streams[2].cardinality.value.requires_exact_row_count());
    assert!(streams[3].cardinality.value.requires_exact_row_count());
}
