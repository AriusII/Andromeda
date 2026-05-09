use andromeda_srpl_diagnostics::{
    DiagnosticPhase, ForbiddenConstruct, ForbiddenConstructHit, SourceSpan, SrplDiagnostic,
    SrplSource, enrich_source_diagnostic, line_column,
};

#[test]
fn owner_diagnostic_constructs_forbidden_hit_with_phase_and_span() {
    let hit = ForbiddenConstructHit::new(ForbiddenConstruct::SelectStar, SourceSpan::new(18, 26));

    let diagnostic = hit.diagnostic();

    assert_eq!(diagnostic.phase, DiagnosticPhase::Binding);
    assert_eq!(diagnostic.location, Some(SourceSpan::new(18, 26)));
    assert!(diagnostic.message.contains("SRPL-FORBID-007"));
}

#[test]
fn owner_source_enrichment_keeps_location_and_adds_context() {
    let source = "procedure Reserve\nbegin select * end";
    let diagnostic = SrplDiagnostic::new(
        DiagnosticPhase::Binding,
        Some(SourceSpan::new(24, 32)),
        "select star",
    );

    let enriched = enrich_source_diagnostic(source, diagnostic, Some("Inventory.Reserve"));

    assert_eq!(line_column(source, 24), (2, 7));
    assert_eq!(enriched.phase, DiagnosticPhase::Binding);
    assert_eq!(enriched.location, Some(SourceSpan::new(24, 32)));
    assert!(enriched.message.contains("procedure Inventory.Reserve"));
    assert!(enriched.message.contains("line 2, column 7"));
}

#[test]
fn owner_source_scans_forbidden_constructs_directly() {
    let source = SrplSource::new("procedure X begin select * from Inventory.Product end;");

    let diagnostics = source.forbidden_construct_diagnostics();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].phase, DiagnosticPhase::Binding);
}
