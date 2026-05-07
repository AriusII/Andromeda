use crate::{ForbiddenConstruct, ForbiddenConstructHit, SourceSpan};

const URL_SCHEME_SEPARATOR: &str = "://";

pub(super) fn scan_forbidden_construct_hits(source: &str) -> Vec<ForbiddenConstructHit> {
    let lexemes = tokenize_for_forbidden_scan(source);
    let mut hits = Vec::new();

    push_url_scheme_substring(
        source,
        &mut hits,
        ForbiddenConstruct::ExternalNetwork,
        &["http", "https"],
    );
    push_substring(
        source,
        &mut hits,
        ForbiddenConstruct::ExternalFilesystem,
        &["file://"],
    );

    for (index, lexeme) in lexemes.iter().enumerate() {
        if let Some((construct, span)) = forbidden_word_hit(lexeme, lexemes.get(index + 1)) {
            push_hit(&mut hits, construct, span);
        }
    }

    hits.sort_by_key(|hit| (hit.span.start, hit.span.end));
    hits
}

fn forbidden_word_hit(
    lexeme: &Lexeme<'_>,
    next: Option<&Lexeme<'_>>,
) -> Option<(ForbiddenConstruct, SourceSpan)> {
    if lexeme.kind != LexKind::Word {
        return None;
    }

    let text = lexeme.text;
    if text.eq_ignore_ascii_case("while") {
        return Some((ForbiddenConstruct::UnboundedWhile, lexeme.span));
    }
    if text.eq_ignore_ascii_case("recursive") {
        return Some((ForbiddenConstruct::FreeRecursion, lexeme.span));
    }
    if text.eq_ignore_ascii_case("call") && next_word_is(next, "self") {
        return Some((
            ForbiddenConstruct::FreeRecursion,
            span_through_next(lexeme, next),
        ));
    }
    if (text.eq_ignore_ascii_case("random") || text.eq_ignore_ascii_case("rand"))
        && next_kind_is(next, LexKind::LParen)
    {
        return Some((
            ForbiddenConstruct::NondeterministicRandom,
            span_through_next(lexeme, next),
        ));
    }
    if text.eq_ignore_ascii_case("select") && next_kind_is(next, LexKind::Star) {
        return Some((
            ForbiddenConstruct::SelectStar,
            span_through_next(lexeme, next),
        ));
    }
    if (text.eq_ignore_ascii_case("dynamic") || text.eq_ignore_ascii_case("execute"))
        && next_word_is(next, "sql")
    {
        return Some((
            ForbiddenConstruct::DynamicTextSql,
            span_through_next(lexeme, next),
        ));
    }
    if text.eq_ignore_ascii_case("external") && next_word_is(next, "filesystem") {
        return Some((
            ForbiddenConstruct::ExternalFilesystem,
            span_through_next(lexeme, next),
        ));
    }

    None
}

fn span_through_next(lexeme: &Lexeme<'_>, next: Option<&Lexeme<'_>>) -> SourceSpan {
    next.map_or(lexeme.span, |next| {
        SourceSpan::new(lexeme.span.start, next.span.end)
    })
}

fn next_word_is(next: Option<&Lexeme<'_>>, expected: &str) -> bool {
    matches!(
        next,
        Some(next) if next.kind == LexKind::Word && next.text.eq_ignore_ascii_case(expected)
    )
}

fn next_kind_is(next: Option<&Lexeme<'_>>, expected: LexKind) -> bool {
    matches!(next, Some(next) if next.kind == expected)
}

fn push_url_scheme_substring(
    source: &str,
    hits: &mut Vec<ForbiddenConstructHit>,
    construct: ForbiddenConstruct,
    schemes: &[&str],
) {
    let lowered = source.to_ascii_lowercase();
    let Some((start, scheme)) = schemes
        .iter()
        .filter_map(|scheme| find_url_scheme_prefix(&lowered, scheme).map(|start| (start, *scheme)))
        .min_by_key(|(start, _)| *start)
    else {
        return;
    };
    push_hit(
        hits,
        construct,
        SourceSpan::new(start, start + scheme.len() + URL_SCHEME_SEPARATOR.len()),
    );
}

fn find_url_scheme_prefix(source: &str, scheme: &str) -> Option<usize> {
    let pattern = format!("{scheme}{URL_SCHEME_SEPARATOR}");
    source.find(&pattern)
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
    if let Some(existing) = hits.iter_mut().find(|h| h.construct == construct) {
        if (span.start, span.end) < (existing.span.start, existing.span.end) {
            existing.span = span;
        }
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
    let mut out = Vec::new();
    let mut chars = input.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        if ch.is_ascii_whitespace() {
            continue;
        }

        if ch.is_ascii_alphanumeric() || ch == '_' {
            let mut end = start + ch.len_utf8();
            while let Some((next_index, next_ch)) = chars.peek().copied() {
                if next_ch.is_ascii_alphanumeric() || next_ch == '_' {
                    chars.next();
                    end = next_index + next_ch.len_utf8();
                } else {
                    break;
                }
            }
            out.push(Lexeme {
                kind: LexKind::Word,
                text: &input[start..end],
                span: SourceSpan::new(start, end),
            });
        } else {
            let kind = match ch {
                '*' => LexKind::Star,
                '(' => LexKind::LParen,
                _ => LexKind::Other,
            };
            let end = start + ch.len_utf8();
            out.push(Lexeme {
                kind,
                text: &input[start..end],
                span: SourceSpan::new(start, end),
            });
        }
    }
    out
}
