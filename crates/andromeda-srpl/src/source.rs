use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{ForbiddenConstruct, ForbiddenConstructHit, SourceSpan, SrplDiagnostic};

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
        let lexemes = tokenize_for_forbidden_scan(self.text);
        let mut hits: Vec<ForbiddenConstructHit> = Vec::new();

        // 1. Substring-anchored constructs whose markers cannot appear inside
        //    a SRPL identifier (they contain `:` or `/`). Word-boundary safe.
        push_substring(
            self.text,
            &mut hits,
            ForbiddenConstruct::ExternalNetwork,
            &["http://", "https://"],
        );
        push_substring(
            self.text,
            &mut hits,
            ForbiddenConstruct::ExternalFilesystem,
            &["file://"],
        );

        // 2. Word/punctuation-structural constructs. We require *whole-word*
        //    matches, so identifiers like `whileCount`, `myRandom`, `brand`,
        //    `BackupFilesystem`, or `executeSqlBuilder` do not false-positive.
        for (i, lex) in lexemes.iter().enumerate() {
            if lex.kind == LexKind::Word {
                let lower = lex.text.to_ascii_lowercase();
                let next = lexemes.get(i + 1);
                let next_word = next.and_then(|n| {
                    if n.kind == LexKind::Word {
                        Some(n.text.to_ascii_lowercase())
                    } else {
                        None
                    }
                });
                let next_is_lparen = matches!(next, Some(n) if n.kind == LexKind::LParen);
                let next_is_star = matches!(next, Some(n) if n.kind == LexKind::Star);

                match lower.as_str() {
                    // UnboundedWhile: any standalone `while` keyword. SRPL
                    // core has no legitimate `while` usage; identifiers
                    // such as `whileCount` are *not* the bare word `while`
                    // and therefore do not match here.
                    "while" => push_hit(&mut hits, ForbiddenConstruct::UnboundedWhile, lex.span),
                    // FreeRecursion: standalone `recursive` keyword, or the
                    // adjacent word pair `call self`.
                    "recursive" => push_hit(&mut hits, ForbiddenConstruct::FreeRecursion, lex.span),
                    "call" if next_word.as_deref() == Some("self") => push_hit(
                        &mut hits,
                        ForbiddenConstruct::FreeRecursion,
                        SourceSpan::new(lex.span.start, next.unwrap().span.end),
                    ),
                    // NondeterministicRandom: `random(` / `rand(` as a
                    // call. The call form is required so identifiers like
                    // `RandomSeed` or `brand` never trip the rule.
                    "random" | "rand" if next_is_lparen => push_hit(
                        &mut hits,
                        ForbiddenConstruct::NondeterministicRandom,
                        SourceSpan::new(lex.span.start, next.unwrap().span.end),
                    ),
                    // SelectStar: `select` keyword followed by `*` (any
                    // amount of whitespace between).
                    "select" if next_is_star => push_hit(
                        &mut hits,
                        ForbiddenConstruct::SelectStar,
                        SourceSpan::new(lex.span.start, next.unwrap().span.end),
                    ),
                    // DynamicTextSql: `dynamic sql` or `execute sql` as
                    // adjacent words (case- and whitespace-insensitive).
                    "dynamic" | "execute" if next_word.as_deref() == Some("sql") => push_hit(
                        &mut hits,
                        ForbiddenConstruct::DynamicTextSql,
                        SourceSpan::new(lex.span.start, next.unwrap().span.end),
                    ),
                    // ExternalFilesystem: adjacent words `external filesystem`.
                    "external" if next_word.as_deref() == Some("filesystem") => push_hit(
                        &mut hits,
                        ForbiddenConstruct::ExternalFilesystem,
                        SourceSpan::new(lex.span.start, next.unwrap().span.end),
                    ),
                    _ => {}
                }
            }
        }

        // Stable order: by source span start so callers receive deterministic
        // diagnostics regardless of the rule that produced them.
        hits.sort_by_key(|hit| (hit.span.start, hit.span.end));
        hits
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

fn push_substring(
    source: &str,
    hits: &mut Vec<ForbiddenConstructHit>,
    construct: ForbiddenConstruct,
    patterns: &[&str],
) {
    let lowered = source.to_ascii_lowercase();
    let Some((start, pattern)) = patterns
        .iter()
        .filter_map(|pattern| lowered.find(pattern).map(|start| (start, *pattern)))
        .min_by_key(|(start, _)| *start)
    else {
        return;
    };
    push_hit(
        hits,
        construct,
        SourceSpan::new(start, start + pattern.len()),
    );
}

fn push_hit(
    hits: &mut Vec<ForbiddenConstructHit>,
    construct: ForbiddenConstruct,
    span: SourceSpan,
) {
    if hits.iter().any(|h| h.construct == construct) {
        // Keep the first structural hit per construct so diagnostics stay
        // focused; later matches are redundant for rejection purposes.
        return;
    }
    hits.push(ForbiddenConstructHit::new(construct, span));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LexKind {
    Word,
    Star,
    LParen,
    Other,
}

#[derive(Debug, Clone)]
struct Lexeme<'a> {
    kind: LexKind,
    text: &'a str,
    span: SourceSpan,
}

/// Coarse word/punctuation tokenizer used solely by the forbidden-construct
/// scanner. It does not attempt to honor SRPL grammar; it only needs to
/// produce stable word boundaries and recognize the punctuation symbols that
/// participate in forbidden patterns (`*`, `(`). Whitespace is skipped so
/// tabs / newlines / multiple spaces between tokens behave identically.
fn tokenize_for_forbidden_scan(input: &str) -> Vec<Lexeme<'_>> {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        if ch.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        if ch.is_ascii_alphanumeric() || ch == '_' {
            i += 1;
            while i < bytes.len() {
                let c = bytes[i] as char;
                if c.is_ascii_alphanumeric() || c == '_' {
                    i += 1;
                } else {
                    break;
                }
            }
            out.push(Lexeme {
                kind: LexKind::Word,
                text: &input[start..i],
                span: SourceSpan::new(start, i),
            });
        } else {
            let kind = match ch {
                '*' => LexKind::Star,
                '(' => LexKind::LParen,
                _ => LexKind::Other,
            };
            i += ch.len_utf8();
            out.push(Lexeme {
                kind,
                text: &input[start..i],
                span: SourceSpan::new(start, i),
            });
        }
    }
    out
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
        assert!(diagnostics[0].message.contains("select star"));
    }

    // ---- False-positive guards -------------------------------------------
    // Identifiers that *contain* a forbidden substring must not be flagged.

    #[test]
    fn identifier_starting_with_while_is_not_unbounded_loop() {
        // `whileCount` and `WhileLimit` are legitimate identifier names; the
        // legacy substring "while " scanner could fire on `someWhile foo`
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

    // ---- Bypass / variant detection --------------------------------------
    // Whitespace and case variations of forbidden constructs must still match.

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
        assert!(net
            .forbidden_constructs()
            .contains(&ForbiddenConstruct::ExternalNetwork));

        let fs_url = SrplSource::new("procedure X begin open file:///etc/passwd end;");
        assert!(fs_url
            .forbidden_constructs()
            .contains(&ForbiddenConstruct::ExternalFilesystem));

        let fs_words = SrplSource::new("procedure X begin call External\tFilesystem end;");
        assert!(fs_words
            .forbidden_constructs()
            .contains(&ForbiddenConstruct::ExternalFilesystem));
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
        assert!(source
            .forbidden_constructs()
            .contains(&ForbiddenConstruct::FreeRecursion));
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
}
