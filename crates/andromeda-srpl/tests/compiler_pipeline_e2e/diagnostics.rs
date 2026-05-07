use crate::support::*;

#[test]
fn parser_diagnostics_include_phase_and_span() {
    let diagnostic =
        parse_procedure_signature("procedure X accepts () returns R many ();").unwrap_err();

    assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
    assert!(diagnostic.location.is_some());
    assert!(diagnostic.location.unwrap().is_valid());
    assert!(diagnostic.message.contains("at least one column"));
}

#[test]
fn compiler_rejects_ambient_absence_and_float_types_with_spans() {
    let cases = [
        (
            "procedure X accepts (ProductId nullable) returns R one (C bool);",
            "forbids nullable values",
        ),
        (
            "procedure X accepts (ProductId optional) returns R one (C bool);",
            "forbids nullable values",
        ),
        (
            "procedure X accepts (ProductId maybe) returns R one (C bool);",
            "forbids nullable values",
        ),
        (
            "procedure X accepts (ProductId i64) returns R one (C null);",
            "forbids nullable values",
        ),
        (
            "procedure X accepts (ProductId i64) returns R one (C option);",
            "forbids nullable values",
        ),
        (
            "procedure X accepts (Price float) returns R one (C bool);",
            "float scalar types are not permitted",
        ),
    ];

    for (source, expected_message) in cases {
        let diagnostic = compile_narrow_procedure_signature(source)
            .expect_err("ambient absence and float types must be rejected");

        assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
        assert!(
            diagnostic.message.contains(expected_message),
            "unexpected diagnostic for {source}: {}",
            diagnostic.message
        );
        let span = diagnostic
            .location
            .expect("type rejection must retain a source span");
        assert!(span.is_valid());
        assert!(source.is_char_boundary(span.start));
        assert!(source.is_char_boundary(span.end));
        assert!(!&source[span.start..span.end].is_empty());
    }
}

#[test]
fn compiler_rejects_absence_keywords_as_identifiers_or_emit_values() {
    let cases = [
        "procedure X accepts (ProductId i64) returns R one (null bool);",
        "procedure X accepts (ProductId i64) returns maybe one (C bool);",
        "procedure X accepts () returns R one (C bool) body { emit R (null); }",
        "procedure X accepts () returns R one (C bool) begin return R (maybe); end;",
    ];

    for source in cases {
        let diagnostic = compile_narrow_procedure_signature(source)
            .expect_err("absence keywords must not be accepted as ordinary SRPL identifiers");

        assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
        assert!(
            diagnostic.message.contains("reserves absence keywords"),
            "unexpected diagnostic for {source}: {}",
            diagnostic.message
        );
        let span = diagnostic
            .location
            .expect("reserved absence keyword diagnostic must retain a source span");
        assert!(span.is_valid());
        assert!(source.is_char_boundary(span.start));
        assert!(source.is_char_boundary(span.end));
    }
}

#[test]
fn forbidden_constructs_reject_before_lowering() {
    let diagnostic = compile_narrow_procedure_signature(
        "procedure X accepts () returns R many (C bool); execute sql",
    )
    .unwrap_err();

    assert_eq!(diagnostic.phase, DiagnosticPhase::Binding);
    assert!(diagnostic.location.is_some());
    assert!(diagnostic.message.contains("SRPL-FORBID-006"));
    assert!(diagnostic.message.contains("dynamic text SQL"));
}

#[test]
fn public_api_is_consumable_from_outside_the_crate() {
    let source = SrplSource::new("procedure X accepts () returns R many (C bool);");
    assert!(source.forbidden_construct_diagnostics().is_empty());

    let span = SourceSpan::new(0, source.text.len());
    assert!(span.is_valid());

    let ir: SrplProcedureIr = andromeda_srpl::compile_narrow_procedure_signature(source.text)
        .expect("root re-export remains public");
    assert_eq!(
        ir.result_streams[0].cardinality,
        andromeda_srpl::Cardinality::Many
    );
}
