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
fn compiler_rejects_dynamic_shapes_with_stable_diagnostics() {
    struct Case {
        source: &'static str,
        phase: DiagnosticPhase,
        expected_message: &'static str,
        expected_span: &'static str,
    }

    let cases = [
        Case {
            source: "procedure Inventory.DynamicTableProbe accepts (TableName text(64)) returns R many (C bool); dynamic SQL",
            phase: DiagnosticPhase::Binding,
            expected_message: "SRPL-FORBID-006",
            expected_span: "dynamic SQL",
        },
        Case {
            source: "procedure Inventory.DynamicPredicateProbe accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) begin ensure Inventory.ProductStock Stock where ProductId = Stock.ProductId or Stock.AvailableQuantity >= Quantity else fail InsufficientStock; return Reservation (Reserved); end;",
            phase: DiagnosticPhase::Parsing,
            expected_message: "expected contextual SRPL keyword and",
            expected_span: "or",
        },
        Case {
            source: "procedure Inventory.ShapeShiftProbe accepts () returns R one (C bool), S one (D bool);",
            phase: DiagnosticPhase::Parsing,
            expected_message: "only one narrow procedure declaration is accepted",
            expected_span: ",",
        },
        Case {
            source: "procedure Inventory.AmbiguousCardinalityProbe accepts () returns R optional many (C bool);",
            phase: DiagnosticPhase::Parsing,
            expected_message: "SRPL optional cardinality must be written as `optional one`",
            expected_span: "optional",
        },
        Case {
            source: "procedure Inventory.NullProbe accepts () returns R optional one (C null);",
            phase: DiagnosticPhase::Parsing,
            expected_message: "SRPL V0 forbids nullable values",
            expected_span: "null",
        },
    ];

    for case in cases {
        let diagnostic = compile_narrow_procedure_signature(case.source)
            .expect_err("SRPL doctrine violation must produce a stable diagnostic");

        assert_eq!(
            diagnostic.phase, case.phase,
            "unexpected diagnostic phase for {}",
            case.source
        );
        assert!(
            diagnostic.message.contains(case.expected_message),
            "unexpected diagnostic for {}: {}",
            case.source,
            diagnostic.message
        );
        assert!(
            diagnostic.message.contains("line 1, column"),
            "source diagnostic must include stable line/column context: {}",
            diagnostic.message
        );
        assert_diagnostic_span(case.source, &diagnostic, case.expected_span);
    }
}

#[test]
fn public_api_is_consumable_from_outside_the_crate() {
    let source = SrplSource::new("procedure X accepts () returns R many (C bool);");
    assert!(source.forbidden_construct_diagnostics().is_empty());

    let span = SourceSpan::new(0, source.text.len());
    assert!(span.is_valid());

    let ir: SrplProcedureIr =
        compile_narrow_procedure_signature(source.text).expect("compiler entrypoint is public");
    assert_eq!(ir.result_streams[0].cardinality, Cardinality::Many);
}

fn assert_diagnostic_span(
    source: &str,
    diagnostic: &andromeda_srpl_diagnostics::SrplDiagnostic,
    expected_span: &str,
) {
    let span = diagnostic
        .location
        .expect("diagnostic must retain a source span");
    assert!(span.is_valid(), "invalid diagnostic span: {span:?}");
    assert!(source.is_char_boundary(span.start));
    assert!(source.is_char_boundary(span.end));
    assert_eq!(&source[span.start..span.end], expected_span);
}
