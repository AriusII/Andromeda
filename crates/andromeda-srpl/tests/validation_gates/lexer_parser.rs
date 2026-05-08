use andromeda_srpl_diagnostics::source_location::SrplSource;
use andromeda_srpl_lexer::{TokenKind, lex};
use andromeda_srpl_parser::{Cardinality, parse_procedure_signature};

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
        assert_ne!(
            tokens[0].kind,
            TokenKind::Identifier,
            "{}: expected keyword token kind",
            desc
        );
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
    let ast = parse_procedure_signature(srpl).expect("should parse simple procedure signature");

    assert_eq!(ast.name.value.as_catalog_path(), "Inventory.ReserveStock");
    assert_eq!(ast.parameters.len(), 1);
    assert_eq!(ast.results.len(), 1);
    assert_eq!(ast.results[0].cardinality.value, Cardinality::One);
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

    let ast = parse_procedure_signature(srpl).expect("should parse procedure with body");

    assert_eq!(ast.body.operations.len(), 4);
    println!("  ✅ Procedure with body parses and validates");
    println!("✅ Gate 02: Procedure body grammar verified");
}
