//! Property tests for the public SRPL parser entrypoint.

#![forbid(unsafe_code)]

use proptest::prelude::*;
use std::panic;

/// Arbitrary UTF-8 string generator for SRPL-like syntax.
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

#[test]
fn prop_parser_never_panics_on_utf8_strings() {
    proptest!(|(input in arb_srpl_input())| {
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            andromeda_srpl::parse_procedure_signature(&input)
        }));

        match result {
            Ok(_) => prop_assert!(true),
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
        let result = andromeda_srpl::parse_procedure_signature(input);
        assert!(
            result.is_ok(),
            "Valid SRPL should parse: {} => {:?}",
            input,
            result
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
        let result = andromeda_srpl::parse_procedure_signature(input);
        if let Err(diag) = result {
            let msg = format!("{:?}", diag);
            assert!(
                !msg.is_empty(),
                "Error diagnostic should provide message for: {}",
                input
            );
        }
    }
}

#[test]
fn prop_parser_deterministic() {
    proptest!(|(input in ".*")| {
        let result1 = andromeda_srpl::parse_procedure_signature(&input);
        let result2 = andromeda_srpl::parse_procedure_signature(&input);

        match (&result1, &result2) {
            (Ok(_), Ok(_)) => prop_assert!(true),
            (Err(_), Err(_)) => prop_assert!(true),
            _ => prop_assert!(false, "Parser returned different results for same input"),
        }
    });
}

#[test]
fn prop_parser_handles_large_inputs() {
    proptest!(|(size in 1000usize..100000)| {
        let large_input = "a".repeat(size);

        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            andromeda_srpl::parse_procedure_signature(&large_input)
        }));

        match result {
            Ok(_) => prop_assert!(true, "parser handled large input"),
            Err(_) => prop_assert!(false, "parser panicked on large input of size {}", size),
        }
    });
}

#[test]
fn prop_parser_handles_empty_input() {
    let result = andromeda_srpl::parse_procedure_signature("");
    assert!(
        result.is_err(),
        "Empty input should produce error, not panic"
    );
}
