#![forbid(unsafe_code)]

use andromeda_srpl_parser::{
    BusinessOperationKindAst, Cardinality, DiagnosticPhase, SourceSpan, TokenKind, lex,
    parse_procedure_signature, source_location,
};
use andromeda_types::{AbsencePolicy, ScalarType};

fn assert_utf8_span(source: &str, span: SourceSpan) {
    assert!(span.is_valid(), "span must be ordered: {span:?}");
    assert!(
        span.end <= source.len(),
        "span must stay inside source bounds: {span:?} for {source:?}"
    );
    assert!(
        source.is_char_boundary(span.start),
        "span start must be a UTF-8 boundary: {span:?} for {source:?}"
    );
    assert!(
        source.is_char_boundary(span.end),
        "span end must be a UTF-8 boundary: {span:?} for {source:?}"
    );
}

#[test]
fn lexer_directly_emits_keywords_punctuation_and_byte_spans() {
    let source = "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);";

    let tokens = lex(source).expect("parser owner lexer must accept a narrow signature");

    assert_eq!(tokens[0].kind, TokenKind::Procedure);
    assert!(tokens.iter().any(|token| token.kind == TokenKind::Dot));
    assert!(tokens.iter().any(|token| token.kind == TokenKind::LParen));
    assert!(tokens.iter().any(|token| token.kind == TokenKind::RParen));
    assert!(
        tokens
            .iter()
            .any(|token| token.kind == TokenKind::Semicolon)
    );
    assert!(tokens.iter().any(|token| token.kind == TokenKind::One));

    for token in tokens {
        assert_utf8_span(source, token.span);
        assert_eq!(
            &source[token.span.start..token.span.end],
            token.lexeme,
            "token lexeme must be a direct source slice"
        );
    }
}

#[test]
fn parser_directly_builds_ast_for_narrow_body_operations() {
    let source = "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) \
        returns Reservation one (Reserved bool) \
        body { \
            read Inventory.ProductStock Stock one; \
            assert Quantity InsufficientStock; \
            update Inventory.ProductStock AvailableQuantity; \
            emit Reservation (Reserved); \
        }";

    let ast = parse_procedure_signature(source)
        .expect("parser owner must parse the bounded body grammar directly");

    assert_eq!(ast.name.value.as_catalog_path(), "Inventory.ReserveStock");
    assert_eq!(ast.parameters.len(), 2);
    assert_eq!(ast.parameters[0].name.value, "ProductId");
    assert_eq!(
        ast.parameters[0].data_type.value.absence,
        AbsencePolicy::Required
    );
    assert_eq!(ast.parameters[0].data_type.value.scalar, ScalarType::I64);
    assert_eq!(ast.results.len(), 1);
    assert_eq!(ast.results[0].name.value, "Reservation");
    assert_eq!(ast.results[0].cardinality.value, Cardinality::One);
    assert_eq!(ast.results[0].columns[0].name.value, "Reserved");
    assert_eq!(ast.body.operations.len(), 4);

    match &ast.body.operations[0].kind {
        BusinessOperationKindAst::Read {
            source,
            binding,
            cardinality,
        } => {
            assert_eq!(source.value.as_catalog_path(), "Inventory.ProductStock");
            assert_eq!(binding.value, "Stock");
            assert_eq!(cardinality.value, Cardinality::One);
        }
        other => panic!("first operation must be Read, got {other:?}"),
    }

    match &ast.body.operations[1].kind {
        BusinessOperationKindAst::Assert {
            predicate,
            failure_code,
        } => {
            assert_eq!(predicate.value, "Quantity");
            assert_eq!(failure_code.value, "InsufficientStock");
        }
        other => panic!("second operation must be Assert, got {other:?}"),
    }

    match &ast.body.operations[2].kind {
        BusinessOperationKindAst::Update {
            target,
            mutation,
            affected_rows_exact,
        } => {
            assert_eq!(target.value.as_catalog_path(), "Inventory.ProductStock");
            assert_eq!(mutation.value, "AvailableQuantity");
            assert!(affected_rows_exact.is_none());
        }
        other => panic!("third operation must be Update, got {other:?}"),
    }

    match &ast.body.operations[3].kind {
        BusinessOperationKindAst::Emit { stream, values } => {
            assert_eq!(stream.value, "Reservation");
            assert_eq!(values.len(), 1);
            assert_eq!(values[0].value, "Reserved");
        }
        other => panic!("fourth operation must be Emit, got {other:?}"),
    }
}

#[test]
fn parser_directly_accepts_documented_cardinality_forms() {
    let cases = [
        ("one", Cardinality::One),
        ("optional one", Cardinality::OptionalOne),
        ("optional_one", Cardinality::OptionalOne),
        ("many", Cardinality::Many),
        ("nonempty many", Cardinality::NonEmptyMany),
        ("non_empty_many", Cardinality::NonEmptyMany),
    ];

    for (cardinality_source, expected) in cases {
        let source = format!(
            "procedure Inventory.Lookup accepts (ProductId i64) returns R {cardinality_source} (C bool);"
        );
        let ast = parse_procedure_signature(&source)
            .expect("documented cardinality form must parse directly");
        let cardinality = &ast.results[0].cardinality;

        assert_eq!(
            cardinality.value, expected,
            "unexpected cardinality for {cardinality_source}"
        );
        assert_utf8_span(&source, cardinality.span);
        assert_eq!(
            &source[cardinality.span.start..cardinality.span.end],
            cardinality_source
        );
    }
}

#[test]
fn parser_directly_rejects_ambiguous_absence_and_float_surface() {
    let cases = [
        (
            "procedure X accepts (P i64) returns R optional many (C bool);",
            "optional cardinality must be written as `optional one`",
        ),
        (
            "procedure X accepts (P i64) returns R nonempty one (C bool);",
            "nonempty cardinality must be written as `nonempty many`",
        ),
        (
            "procedure X accepts (ProductId nullable) returns R one (C bool);",
            "forbids nullable values",
        ),
        (
            "procedure X accepts (Price float) returns R one (C bool);",
            "float scalar types are not permitted",
        ),
        (
            "procedure X accepts (ProductId i64) returns R one (null bool);",
            "reserves absence keywords",
        ),
    ];

    for (source, expected_message) in cases {
        let diagnostic = parse_procedure_signature(source)
            .expect_err("ambiguous or forbidden SRPL surface must fail closed");

        assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
        assert!(
            diagnostic.message.contains(expected_message),
            "unexpected diagnostic for {source}: {}",
            diagnostic.message
        );
        let span = diagnostic
            .location
            .expect("parser diagnostic must carry a source span");
        assert_utf8_span(source, span);
        assert!(!&source[span.start..span.end].is_empty());
    }
}

#[test]
fn lexer_directly_reports_utf8_safe_diagnostic_spans() {
    let source = "procedure X accepts () returns R one (C bool); \u{1f60a}";

    let diagnostic = lex(source).expect_err("emoji is not an SRPL token");

    assert_eq!(diagnostic.phase, DiagnosticPhase::Lexing);
    let span = diagnostic
        .location
        .expect("lexing diagnostic must include a source span");
    assert_utf8_span(source, span);
    assert_eq!(&source[span.start..span.end], "\u{1f60a}");
}

#[test]
fn parser_directly_enforces_bounded_body_operation_limit() {
    let accepted_source = body_with_emit_count(16);
    let accepted_ast = parse_procedure_signature(&accepted_source)
        .expect("sixteen operations must fit the parser owner bound");
    assert_eq!(accepted_ast.body.operations.len(), 16);

    let rejected_source = body_with_emit_count(17);
    let diagnostic = parse_procedure_signature(&rejected_source)
        .expect_err("seventeen operations must exceed the parser owner bound");

    assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
    assert!(
        diagnostic.message.contains("bounded operation limit"),
        "unexpected bounded operation diagnostic: {}",
        diagnostic.message
    );
    let span = diagnostic
        .location
        .expect("operation-limit diagnostic must include a source span");
    assert_utf8_span(&rejected_source, span);
}

#[test]
fn parser_owner_exports_source_location_boundary() {
    let span = source_location::SourceSpan::new(2, 5);
    assert_eq!(span.len(), 3);
    assert!(span.is_valid());

    let source = source_location::SrplSource::new(
        "procedure Inventory.Lookup accepts () returns R one (C bool);",
    );
    assert!(source.forbidden_construct_diagnostics().is_empty());
}

fn body_with_emit_count(count: usize) -> String {
    let operations = (0..count)
        .map(|_| "emit R (C);")
        .collect::<Vec<_>>()
        .join(" ");

    format!("procedure X accepts () returns R many (C bool) body {{ {operations} }}")
}
