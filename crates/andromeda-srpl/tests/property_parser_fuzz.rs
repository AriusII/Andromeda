//! Property tests for the public SRPL parser entrypoint.

#![forbid(unsafe_code)]

use proptest::prelude::*;
use std::panic;

use andromeda_srpl_ast::ProcedureAst;
use andromeda_srpl_diagnostics::SrplDiagnostic;
use andromeda_srpl_ir::Cardinality;
use andromeda_srpl_parser::parse_procedure_signature;

fn arb_srpl_input() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);".to_string()),
        Just("procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) body { read Inventory.ProductStock Stock one; emit Reservation (Reserved); }".to_string()),
        Just("PROCEDURE p(".to_string()),
        Just("BEGIN SELECT".to_string()),
        Just("PROCEDURE p()".to_string()),
        "\\PC{0,1000}".prop_map(|s| s),
        "[\\x20-\\x7E]{0,500}".prop_map(|s| s)
    ]
}

fn diagnostic_has_stable_shape(diagnostic: &SrplDiagnostic, input_len: usize) -> bool {
    !diagnostic.message.trim().is_empty()
        && diagnostic
            .location
            .is_none_or(|span| span.is_valid() && span.end <= input_len)
}

fn ast_has_stable_shape(ast: &ProcedureAst, input_len: usize) -> bool {
    ast.span.is_valid()
        && ast.span.end <= input_len
        && ast.name.span.is_valid()
        && ast.name.span.end <= input_len
        && !ast.name.value.parts().is_empty()
        && ast
            .parameters
            .iter()
            .enumerate()
            .all(|(index, field)| field.ordinal == index as u32 && !field.name.value.is_empty())
        && ast.results.iter().all(|stream| {
            stream.span.is_valid()
                && stream.span.end <= input_len
                && !stream.name.value.is_empty()
                && stream.columns.iter().enumerate().all(|(index, field)| {
                    field.ordinal == index as u32 && !field.name.value.is_empty()
                })
        })
        && ast
            .body
            .operations
            .iter()
            .enumerate()
            .all(|(index, operation)| operation.ordinal == index as u32)
}

fn parse_result_has_stable_shape(
    result: &Result<ProcedureAst, SrplDiagnostic>,
    input_len: usize,
) -> bool {
    match result {
        Ok(ast) => ast_has_stable_shape(ast, input_len),
        Err(diagnostic) => diagnostic_has_stable_shape(diagnostic, input_len),
    }
}

#[test]
fn prop_parser_never_panics_on_utf8_strings() {
    proptest!(|(input in arb_srpl_input())| {
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            parse_procedure_signature(&input)
        }));

        match result {
            Ok(parse_result) => prop_assert!(
                parse_result_has_stable_shape(&parse_result, input.len()),
                "parser returned malformed AST/diagnostic for input: {:?} => {:?}",
                input,
                parse_result
            ),
            Err(_) => prop_assert!(false, "parser panicked on input: {:?}", input),
        }
    });
}

#[test]
fn prop_valid_srpl_parses_successfully() {
    let valid_cases = vec![
        "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);",
        "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) body { read Inventory.ProductStock Stock one; emit Reservation (Reserved); }",
        "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) body { read Inventory.ProductStock Stock one; assert Quantity InsufficientStock; update Inventory.ProductStock AvailableQuantity; emit Reservation (Reserved); }",
    ];

    for input in valid_cases {
        let result = parse_procedure_signature(input);
        let ast =
            result.unwrap_or_else(|err| panic!("valid SRPL should parse: {input} => {err:?}"));

        assert_eq!(ast.name.value.as_catalog_path(), "Inventory.ReserveStock");
        assert_eq!(ast.results.len(), 1);
        assert_eq!(ast.results[0].name.value, "Reservation");
        assert_eq!(ast.results[0].cardinality.value, Cardinality::One);
        assert_eq!(ast.results[0].columns.len(), 1);
        assert_eq!(ast.results[0].columns[0].name.value, "Reserved");
        assert!(
            ast_has_stable_shape(&ast, input.len()),
            "valid SRPL produced malformed AST: {ast:?}"
        );
    }
}

#[test]
fn prop_invalid_srpl_returns_error_with_message() {
    let invalid_cases = vec![
        "PROCEDURE p(",        // Truncated
        "BEGIN SELECT",        // No PROCEDURE
        "SELECT 1;",           // Not a procedure
        "PROCEDURE",           // Incomplete
        "PROCEDURE p() BEGIN", // Missing END
    ];

    for input in invalid_cases {
        let result = parse_procedure_signature(input);
        let diag = result.unwrap_err();

        assert!(
            diagnostic_has_stable_shape(&diag, input.len()),
            "invalid SRPL should produce a bounded diagnostic for {input}: {diag:?}"
        );
    }
}

#[test]
fn prop_parser_deterministic() {
    proptest!(|(input in ".*")| {
        let result1 = parse_procedure_signature(&input);
        let result2 = parse_procedure_signature(&input);

        prop_assert_eq!(
            &result1,
            &result2,
            "parser must return deterministic AST/diagnostic values for the same input"
        );
        prop_assert!(
            parse_result_has_stable_shape(&result1, input.len()),
            "parser returned malformed AST/diagnostic for input: {:?} => {:?}",
            input,
            result1
        );
    });
}

#[test]
fn prop_parser_handles_large_inputs() {
    proptest!(|(size in 1000usize..100000)| {
        let large_input = "a".repeat(size);

        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            parse_procedure_signature(&large_input)
        }));

        match result {
            Ok(parse_result) => prop_assert!(
                parse_result_has_stable_shape(&parse_result, large_input.len()),
                "parser returned malformed AST/diagnostic for large input size {}: {:?}",
                size,
                parse_result
            ),
            Err(_) => prop_assert!(false, "parser panicked on large input of size {}", size),
        }
    });
}

#[test]
fn prop_parser_handles_empty_input() {
    let result = parse_procedure_signature("");
    let diag = result.unwrap_err();

    assert!(
        diagnostic_has_stable_shape(&diag, 0),
        "empty input should produce a bounded diagnostic: {diag:?}"
    );
}
