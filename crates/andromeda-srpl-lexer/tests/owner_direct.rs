use andromeda_srpl_diagnostics::DiagnosticPhase;
use andromeda_srpl_lexer::{SourceSpan, TokenKind, lex};

fn assert_span_matches_source(source: &str, span: SourceSpan, expected: &str) {
    assert!(span.is_valid());
    assert!(source.is_char_boundary(span.start));
    assert!(source.is_char_boundary(span.end));
    assert_eq!(&source[span.start..span.end], expected);
}

#[test]
fn owner_lexer_emits_public_tokens_with_source_spans() {
    let source =
        "procedure Inventory.ReserveStock accepts () returns Reservation one (Reserved bool);";

    let tokens = lex(source).unwrap();

    assert_eq!(tokens[0].kind, TokenKind::Procedure);
    assert!(tokens.iter().any(|token| token.kind == TokenKind::Dot));
    assert!(tokens.iter().any(|token| token.kind == TokenKind::One));
    assert_span_matches_source(source, tokens[0].span, "procedure");
}

#[test]
fn owner_lexer_reports_lexing_diagnostic_for_unsupported_character() {
    let source = "procedure X accepts () returns R one (C bool); @";

    let diagnostic = lex(source).unwrap_err();

    assert_eq!(diagnostic.phase, DiagnosticPhase::Lexing);
    let span = diagnostic.location.unwrap();
    assert_span_matches_source(source, span, "@");
}
