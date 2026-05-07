use crate::support::assert_compile_path_does_not_panic;

#[test]
fn gate_05_error_path_invalid_syntax_no_panic() {
    let invalid_cases = [
        ("procedure", "incomplete procedure"),
        ("accepts () returns", "incomplete signature"),
        ("procedure X accepts () returns", "missing body"),
    ];

    for (srpl, desc) in invalid_cases {
        assert_compile_path_does_not_panic(srpl, desc, "invalid syntax case");
        println!("  ✅ {}: no panic on invalid input", desc);
    }

    println!("✅ Gate 05: No panics in error paths");
}

#[test]
fn gate_05_error_path_semantic_errors_no_panic() {
    let semantic_errors = [
        (
            "procedure X accepts (P i64) returns R many ();",
            "empty column list",
        ),
        (
            "procedure X accepts () returns R one (C bool) body { read Invalid.Table T one; }",
            "invalid table",
        ),
    ];

    for (srpl, desc) in semantic_errors {
        assert_compile_path_does_not_panic(srpl, desc, "semantic error case");
        println!("  ✅ {}: no panic", desc);
    }

    println!("✅ Gate 05: No panics in semantic error paths");
}
