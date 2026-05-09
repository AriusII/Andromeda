#![forbid(unsafe_code)]

use andromeda_srpl_diagnostics::source_location::SrplSource;
use andromeda_srpl_lexer::{TokenKind, lex};
use andromeda_srpl_parser::{Cardinality, parse_procedure_signature};

#[test]
fn lexer_owner_covers_keyword_tokens() {
    let keywords = ["procedure", "accepts", "returns", "one", "many"];

    for keyword in keywords {
        let source = SrplSource::new(keyword);
        let tokens = lex(source.text).expect("keyword must lex");
        assert!(!tokens.is_empty(), "no tokens produced for {keyword}");
        assert_ne!(
            tokens[0].kind,
            TokenKind::Identifier,
            "keyword must not lex as a plain identifier: {keyword}"
        );
    }
}

#[test]
fn lexer_owner_covers_punctuation_tokens() {
    for punctuation in [".", ",", "(", ")", "{", "}", ";", "="] {
        lex(punctuation).unwrap_or_else(|_| panic!("failed to lex punctuation {punctuation}"));
    }
}

#[test]
fn parser_owner_parses_simple_procedure_signature() {
    let srpl = "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);";
    let ast = parse_procedure_signature(srpl).expect("simple procedure signature must parse");

    assert_eq!(ast.name.value.as_catalog_path(), "Inventory.ReserveStock");
    assert_eq!(ast.parameters.len(), 1);
    assert_eq!(ast.results.len(), 1);
    assert_eq!(ast.results[0].cardinality.value, Cardinality::One);
}

#[test]
fn parser_owner_parses_procedure_with_body() {
    let srpl = "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) \
        returns Reservation one (Reserved bool) \
        body { \
            read Inventory.ProductStock Stock one; \
            assert Quantity InsufficientStock; \
            update Inventory.ProductStock AvailableQuantity; \
            emit Reservation (Reserved); \
        }";

    let ast = parse_procedure_signature(srpl).expect("procedure with body must parse");

    assert_eq!(ast.body.operations.len(), 4);
}
