#![forbid(unsafe_code)]

use andromeda_error::AndromedaErrorKind;
use andromeda_procedure_contract::QualifiedName;
use andromeda_srpl_ast::{
    BusinessOperationAst, BusinessOperationKindAst, Cardinality, ProcedureBodyAst, SourceSpan,
    Spanned,
};
use andromeda_srpl_ir::{
    MAX_SRPL_BODY_OPERATIONS, SrplBusinessOperationKindIr, SrplPredicateIr, SrplValueIr,
};
use andromeda_srpl_lowering::lower_body_ast;

#[test]
fn owner_direct_body_lowering_expands_business_operations_without_srpl_facade() {
    let ir = lower_body_ast(ProcedureBodyAst {
        operations: vec![
            BusinessOperationAst {
                ordinal: 0,
                kind: BusinessOperationKindAst::Ensure {
                    source: qname("Inventory.ProductStock"),
                    binding: ident("Stock"),
                    lookup_input: ident("ProductId"),
                    lookup_field: ident("ProductId"),
                    quantity_field: ident("AvailableQuantity"),
                    quantity_input: ident("Quantity"),
                    failure_code: ident("InsufficientStock"),
                },
                span: span(),
            },
            BusinessOperationAst {
                ordinal: 1,
                kind: BusinessOperationKindAst::UpdateSet {
                    target: qname("Inventory.ProductStock"),
                    field: ident("AvailableQuantity"),
                    value_binding: ident("Stock"),
                    value_field: ident("AvailableQuantity"),
                    value_input: ident("Quantity"),
                    where_input: ident("ProductId"),
                    where_binding: ident("Stock"),
                    where_field: ident("ProductId"),
                    affected_rows_exact: Spanned::new(1, span()),
                },
                span: span(),
            },
        ],
        span: span(),
    })
    .expect("lowering owner must lower bounded body AST directly");

    assert_eq!(ir.operations.len(), 3);
    assert!(matches!(
        &ir.operations[0].kind,
        SrplBusinessOperationKindIr::Read {
            binding,
            cardinality: Cardinality::One,
            predicates,
            ..
        } if binding == "Stock" && matches!(
            predicates.first(),
            Some(SrplPredicateIr::InputEqualsField { input, field, .. })
                if input == "ProductId" && field == "ProductId"
        )
    ));
    assert!(matches!(
        &ir.operations[1].kind,
        SrplBusinessOperationKindIr::Assert { failure_code, .. }
            if failure_code == "InsufficientStock"
    ));
    assert!(matches!(
        &ir.operations[2].kind,
        SrplBusinessOperationKindIr::Update {
            assignments,
            affected_rows_exact: Some(1),
            ..
        } if matches!(
            assignments.first().map(|assignment| &assignment.value),
            Some(SrplValueIr::SubtractInput { binding, field, input })
                if binding == "Stock" && field == "AvailableQuantity" && input == "Quantity"
        )
    ));
    assert!(ir.validate_bounded().is_ok());
}

#[test]
fn owner_direct_body_lowering_rejects_operations_beyond_owner_bound() {
    let operations = (0..=MAX_SRPL_BODY_OPERATIONS)
        .map(|ordinal| BusinessOperationAst {
            ordinal: ordinal as u32,
            kind: BusinessOperationKindAst::Raise {
                code: ident("TooManyOperations"),
            },
            span: span(),
        })
        .collect();

    let error = lower_body_ast(ProcedureBodyAst {
        operations,
        span: span(),
    })
    .expect_err("lowering owner must enforce bounded body operation count");

    assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
    assert!(error.message().contains("bounded operation limit"));
}

fn ident(value: &str) -> Spanned<String> {
    Spanned::new(value.to_string(), span())
}

fn qname(value: &str) -> Spanned<QualifiedName> {
    Spanned::new(QualifiedName::parse(value).unwrap(), span())
}

fn span() -> SourceSpan {
    SourceSpan::new(0, 1)
}
