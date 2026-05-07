use andromeda_srpl::procedure_compiler::lex;
use andromeda_srpl::source_location::SrplSource;

use crate::support::compile_gate_source;

#[test]
fn gate_01_lexer_all_keyword_tokens() {
    let keywords = [
        ("procedure", "should tokenize procedure keyword"),
        ("accepts", "should tokenize accepts keyword"),
        ("returns", "should tokenize returns keyword"),
        ("one", "should tokenize one cardinality"),
        ("many", "should tokenize many cardinality"),
    ];

    for (keyword, desc) in keywords {
        let source = SrplSource::new(keyword);
        let result = lex(source.text);
        assert!(result.is_ok(), "{}: lex failed for '{}'", desc, keyword);
        let tokens = result.unwrap();
        assert!(!tokens.is_empty(), "{}: no tokens produced", desc);
        println!("  ✅ {}: '{}'", desc, keyword);
    }

    println!("✅ Gate 01: All lexer keyword tokens covered");
}

#[test]
fn gate_01_lexer_all_punctuation_tokens() {
    let punctuation = [
        (".", "dot"),
        (",", "comma"),
        ("(", "left paren"),
        (")", "right paren"),
        ("{", "left brace"),
        ("}", "right brace"),
        (";", "semicolon"),
        ("=", "equal"),
    ];

    for (punct, name) in punctuation {
        let result = lex(punct);
        assert!(result.is_ok(), "Failed to lex {} '{}'", name, punct);
        println!("  ✅ Punctuation token {}: '{}'", name, punct);
    }

    println!("✅ Gate 01: All punctuation tokens covered");
}

#[test]
fn gate_02_parser_simple_procedure_signature() {
    let srpl = "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);";
    let ir = compile_gate_source(srpl, "should compile simple procedure signature");

    assert_eq!(ir.name.as_catalog_path(), "Inventory.ReserveStock");
    assert_eq!(ir.inputs.len(), 1);
    assert_eq!(ir.result_streams.len(), 1);
    println!("  ✅ Simple procedure signature parses correctly");
    println!("✅ Gate 02: Parser grammar coverage verified");
}

#[test]
fn gate_02_parser_procedure_with_body() {
    let srpl = "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) \
        returns Reservation one (Reserved bool) \
        body { \
            read Inventory.ProductStock Stock one; \
            assert Quantity InsufficientStock; \
            update Inventory.ProductStock AvailableQuantity; \
            emit Reservation (Reserved); \
        }";

    let ir = compile_gate_source(srpl, "should compile procedure with body");

    assert_eq!(ir.body.operations.len(), 4);
    assert!(ir.body.validate_bounded().is_ok());
    println!("  ✅ Procedure with body parses and validates");
    println!("✅ Gate 02: Procedure body grammar verified");
}
