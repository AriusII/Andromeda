use crate::support::*;

#[test]
fn parser_accepts_documented_cardinality_phrases_with_stable_spans() {
    let optional_source = "procedure X accepts (P i64) returns R optional one (C bool);";
    let optional_ast = parse_procedure_signature(optional_source).unwrap();
    let optional_cardinality = &optional_ast.results[0].cardinality;

    assert_eq!(optional_cardinality.value, Cardinality::OptionalOne);
    assert_eq!(
        &optional_source[optional_cardinality.span.start..optional_cardinality.span.end],
        "optional one"
    );

    let nonempty_source = "procedure X accepts (P i64) returns R nonempty many (C bool);";
    let nonempty_ast = parse_procedure_signature(nonempty_source).unwrap();
    let nonempty_cardinality = &nonempty_ast.results[0].cardinality;

    assert_eq!(nonempty_cardinality.value, Cardinality::NonEmptyMany);
    assert_eq!(
        &nonempty_source[nonempty_cardinality.span.start..nonempty_cardinality.span.end],
        "nonempty many"
    );
}

#[test]
fn parser_rejects_ambiguous_cardinality_phrases_with_stable_diagnostics() {
    let cases = [
        (
            "procedure X accepts (P i64) returns R optional many (C bool);",
            "SRPL optional cardinality must be written as `optional one`",
        ),
        (
            "procedure X accepts (P i64) returns R nonempty one (C bool);",
            "SRPL nonempty cardinality must be written as `nonempty many`",
        ),
        (
            "procedure X accepts (P i64) returns R optional (C bool);",
            "SRPL optional cardinality must be written as `optional one`",
        ),
    ];

    for (source, expected_message) in cases {
        let diagnostic = parse_procedure_signature(source)
            .expect_err("ambiguous cardinality phrase must fail closed");

        assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
        assert!(
            diagnostic.message.contains(expected_message),
            "unexpected diagnostic for {source}: {}",
            diagnostic.message
        );
        let span = diagnostic
            .location
            .expect("cardinality diagnostic must retain a source span");
        assert!(span.is_valid());
        assert!(source.is_char_boundary(span.start));
        assert!(source.is_char_boundary(span.end));
    }
}

#[test]
fn lexer_error_span_is_valid_utf8_boundary() {
    let source = "procedure X accepts () returns R one (C bool); 😊";
    let diagnostic = andromeda_srpl::procedure_compiler::lex(source).unwrap_err();

    assert_eq!(diagnostic.phase, DiagnosticPhase::Lexing);
    let span = diagnostic
        .location
        .expect("lexing diagnostic must include a byte span");
    assert!(span.is_valid());
    assert!(source.is_char_boundary(span.start));
    assert!(source.is_char_boundary(span.end));
    assert_eq!(&source[span.start..span.end], "😊");
}
