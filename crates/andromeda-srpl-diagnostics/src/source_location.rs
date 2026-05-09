//! SRPL source text and location primitives.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{ForbiddenConstruct, ForbiddenConstructHit, SrplDiagnostic};

mod forbidden_scan;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

impl SourceSpan {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub const fn is_valid(self) -> bool {
        self.start <= self.end
    }

    pub const fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub const fn is_empty(self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplSource<'a> {
    pub text: &'a str,
}

impl<'a> SrplSource<'a> {
    pub const fn new(text: &'a str) -> Self {
        Self { text }
    }

    pub fn forbidden_constructs(&self) -> Vec<ForbiddenConstruct> {
        self.forbidden_construct_hits()
            .into_iter()
            .map(|hit| hit.construct)
            .collect()
    }

    pub fn forbidden_construct_hits(&self) -> Vec<ForbiddenConstructHit> {
        forbidden_scan::scan_forbidden_construct_hits(self.text)
    }

    pub fn forbidden_construct_diagnostics(&self) -> Vec<SrplDiagnostic> {
        self.forbidden_construct_hits()
            .into_iter()
            .map(ForbiddenConstructHit::diagnostic)
            .collect()
    }

    pub fn validate_for_core_language(&self) -> AndromedaResult<()> {
        let diagnostics = self.forbidden_construct_diagnostics();
        if diagnostics.is_empty() {
            return Ok(());
        }

        let first = &diagnostics[0];
        let location = first
            .location
            .map(|span| format!(" at byte {}..{}", span.start, span.end))
            .unwrap_or_default();

        Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            format!("{}{}", first.message, location),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srpl_core_rejects_unbounded_and_dynamic_constructs() {
        let source =
            SrplSource::new("procedure X accepts A i64 begin while true do execute sql end;");
        let constructs = source.forbidden_constructs();

        assert!(constructs.contains(&ForbiddenConstruct::UnboundedWhile));
        assert!(constructs.contains(&ForbiddenConstruct::DynamicTextSql));
        assert_eq!(
            source.validate_for_core_language().unwrap_err().kind(),
            AndromedaErrorKind::Srpl
        );
    }

    #[test]
    fn srpl_forbidden_construct_diagnostics_include_conceptual_span() {
        let source = SrplSource::new("procedure X begin select * from Inventory.Product end;");
        let diagnostics = source.forbidden_construct_diagnostics();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].phase, crate::DiagnosticPhase::Binding);
        assert_eq!(diagnostics[0].location, Some(SourceSpan::new(18, 26)));
        assert!(diagnostics[0].message.starts_with("SRPL-FORBID-007"));
        assert!(diagnostics[0].message.contains("select star"));
    }

    #[test]
    fn identifier_starting_with_while_is_not_unbounded_loop() {
        // `whileCount` and `WhileLimit` are legitimate identifier names; the
        // Previous substring "while " scanner could fire on `someWhile foo`
        // which lower-cases to `somewhile foo` and contains "while ".
        let cases = [
            "procedure X accepts (whileCount i64) returns R one (V bool);",
            "procedure X accepts (WhileLimit i64) returns R one (V bool);",
            "procedure X accepts (someWhile i64, then i64) returns R one (V bool);",
        ];
        for case in cases {
            let source = SrplSource::new(case);
            assert!(
                source.forbidden_construct_diagnostics().is_empty(),
                "false positive on UnboundedWhile for: {case}"
            );
        }
    }

    #[test]
    fn identifier_resembling_random_is_not_nondeterministic() {
        // `myRandomSeed`, `RandomTag`, and `brand(x)` must not match because
        // the structural rule requires the bare word `random`/`rand` to be
        // immediately followed by a `(` call.
        let cases = [
            "procedure X accepts (myRandomSeed i64) returns R one (V bool);",
            "procedure X accepts (RandomTag i64) returns R one (V bool);",
            "procedure X accepts (brand i64) returns R one (V bool);",
        ];
        for case in cases {
            let source = SrplSource::new(case);
            assert!(
                source.forbidden_construct_diagnostics().is_empty(),
                "false positive on NondeterministicRandom for: {case}"
            );
        }
    }

    #[test]
    fn identifier_resembling_filesystem_or_recursion_is_not_flagged() {
        // `BackupFilesystem` is a single identifier word, never adjacent to
        // a `external` keyword. `recursiveDispatcher` is *not* the bare word
        // `recursive`.
        let cases = [
            "procedure X accepts (BackupFilesystem i64) returns R one (V bool);",
            "procedure X accepts (recursiveDispatcher i64) returns R one (V bool);",
            "procedure X accepts (selectiveStarRating i64) returns R one (V bool);",
            "procedure X accepts (executeSqlBuilder i64) returns R one (V bool);",
        ];
        for case in cases {
            let source = SrplSource::new(case);
            assert!(
                source.forbidden_construct_diagnostics().is_empty(),
                "false positive for: {case}"
            );
        }
    }

    #[test]
    fn case_variants_of_dynamic_sql_are_caught() {
        let cases = [
            "procedure X accepts () returns R many (C bool); EXECUTE SQL",
            "procedure X accepts () returns R many (C bool); Execute   Sql",
            "procedure X accepts () returns R many (C bool); dynamic\tSQL",
            "procedure X accepts () returns R many (C bool); DYNAMIC\nSQL",
        ];
        for case in cases {
            let source = SrplSource::new(case);
            let cs = source.forbidden_constructs();
            assert!(
                cs.contains(&ForbiddenConstruct::DynamicTextSql),
                "missed DynamicTextSql in: {case}"
            );
        }
    }

    #[test]
    fn whitespace_variants_of_select_star_are_caught() {
        let cases = [
            "procedure X begin SELECT  * from T end;",
            "procedure X begin select\n* from T end;",
            "procedure X begin Select\t* from T end;",
        ];
        for case in cases {
            let source = SrplSource::new(case);
            assert!(
                source
                    .forbidden_constructs()
                    .contains(&ForbiddenConstruct::SelectStar),
                "missed SelectStar in: {case}"
            );
        }
    }

    #[test]
    fn random_call_variants_are_caught() {
        let cases = [
            (
                "procedure X begin v = Random( ) end;",
                ForbiddenConstruct::NondeterministicRandom,
            ),
            (
                "procedure X begin v = RAND(seed) end;",
                ForbiddenConstruct::NondeterministicRandom,
            ),
            (
                "procedure X begin v = random  (1) end;",
                ForbiddenConstruct::NondeterministicRandom,
            ),
        ];
        for (case, expected) in cases {
            let source = SrplSource::new(case);
            assert!(
                source.forbidden_constructs().contains(&expected),
                "missed {expected:?} in: {case}"
            );
        }
    }

    #[test]
    fn external_filesystem_and_network_variants_are_caught() {
        let net = SrplSource::new("procedure X begin fetch HTTPS://example.com end;");
        assert!(
            net.forbidden_constructs()
                .contains(&ForbiddenConstruct::ExternalNetwork)
        );

        let fs_url = SrplSource::new("procedure X begin open file:///etc/passwd end;");
        assert!(
            fs_url
                .forbidden_constructs()
                .contains(&ForbiddenConstruct::ExternalFilesystem)
        );

        let fs_words = SrplSource::new("procedure X begin call External\tFilesystem end;");
        assert!(
            fs_words
                .forbidden_constructs()
                .contains(&ForbiddenConstruct::ExternalFilesystem)
        );
    }

    #[test]
    fn external_filesystem_diagnostic_prefers_earliest_source_span() {
        let source = SrplSource::new("procedure X begin external filesystem then file:///tmp end;");
        let diagnostics = source.forbidden_construct_diagnostics();

        assert_eq!(diagnostics.len(), 1);
        let span = diagnostics[0]
            .location
            .expect("filesystem diagnostic must have a byte span");
        assert_eq!(&source.text[span.start..span.end], "external filesystem");
    }

    #[test]
    fn standalone_while_keyword_is_caught_regardless_of_spacing() {
        let cases = [
            "procedure X begin WHILE true do nothing end;",
            "procedure X begin while\n  true do nothing end;",
            "procedure X begin (while) end;",
        ];
        for case in cases {
            let source = SrplSource::new(case);
            assert!(
                source
                    .forbidden_constructs()
                    .contains(&ForbiddenConstruct::UnboundedWhile),
                "missed UnboundedWhile in: {case}"
            );
        }
    }

    #[test]
    fn free_recursion_word_pair_call_self_is_caught() {
        let source = SrplSource::new("procedure X begin Call   Self end;");
        assert!(
            source
                .forbidden_constructs()
                .contains(&ForbiddenConstruct::FreeRecursion)
        );
    }

    #[test]
    fn diagnostics_carry_phase_and_span() {
        let source = SrplSource::new("procedure X begin EXECUTE SQL end;");
        let diags = source.forbidden_construct_diagnostics();
        assert_eq!(diags.len(), 1);
        let d = &diags[0];
        assert_eq!(d.phase, crate::DiagnosticPhase::Binding);
        let span = d.location.expect("dynamic sql diagnostic must have a span");
        // Span must cover the literal `EXECUTE SQL` substring.
        assert_eq!(&source.text[span.start..span.end], "EXECUTE SQL");
    }

    #[test]
    fn forbidden_scanner_is_utf8_safe_and_keeps_byte_spans() {
        let source = SrplSource::new("procedure Réserve begin note 😊; select * end;");
        let diagnostics = source.forbidden_construct_diagnostics();

        assert_eq!(diagnostics.len(), 1);
        let span = diagnostics[0]
            .location
            .expect("select star diagnostic must have a byte span");
        assert!(span.is_valid());
        assert!(source.text.is_char_boundary(span.start));
        assert!(source.text.is_char_boundary(span.end));
        assert_eq!(&source.text[span.start..span.end], "select *");
    }

    #[test]
    fn forbidden_scanner_accepts_non_ascii_without_forbidden_constructs() {
        let source =
            SrplSource::new("procedure Réserve accepts () returns Résultat one (Valide bool);");

        assert!(source.forbidden_construct_diagnostics().is_empty());
    }

    #[test]
    fn forbidden_scanner_keeps_all_utf8_diagnostic_spans_bounded() {
        let source = SrplSource::new(
            "procedure Réserve begin note 😊; dynamic SQL; https://example.invalid; select *; while true end;",
        );
        let diagnostics = source.forbidden_construct_diagnostics();

        assert_eq!(diagnostics.len(), 4);
        for diagnostic in diagnostics {
            let span = diagnostic
                .location
                .expect("forbidden diagnostic must have a byte span");
            assert!(span.is_valid());
            assert!(span.end <= source.text.len());
            assert!(source.text.is_char_boundary(span.start));
            assert!(source.text.is_char_boundary(span.end));
            assert!(!&source.text[span.start..span.end].is_empty());
        }
    }
}
