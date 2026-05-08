#![forbid(unsafe_code)]

//! SRPL lexical tokens and scanner.
//!
//! This crate owns tokenization only. It does not parse, bind, lower, execute,
//! or consult catalog storage.

use andromeda_srpl_diagnostics::{DiagnosticPhase, SourceSpan, SrplDiagnostic};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Procedure,
    Accepts,
    Returns,
    One,
    OptionalOne,
    Many,
    NonEmptyMany,
    Identifier,
    Dot,
    Comma,
    Equal,
    GreaterEqual,
    Minus,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Semicolon,
    Number,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub lexeme: String,
    pub span: SourceSpan,
}

pub fn lex(input: &str) -> Result<Vec<Token>, SrplDiagnostic> {
    let mut tokens = Vec::new();
    let mut chars = input.char_indices().peekable();

    while let Some((start, ch)) = chars.next() {
        if ch.is_whitespace() {
            continue;
        }

        let (kind, end) = match ch {
            '.' => (TokenKind::Dot, start + ch.len_utf8()),
            ',' => (TokenKind::Comma, start + ch.len_utf8()),
            '=' => (TokenKind::Equal, start + ch.len_utf8()),
            '>' => {
                if let Some((next_index, '=')) = chars.peek().copied() {
                    chars.next();
                    (TokenKind::GreaterEqual, next_index + '='.len_utf8())
                } else {
                    return Err(SrplDiagnostic::new(
                        DiagnosticPhase::Lexing,
                        Some(SourceSpan::new(start, start + ch.len_utf8())),
                        "unexpected character in SRPL procedure signature",
                    ));
                }
            }
            '-' => (TokenKind::Minus, start + ch.len_utf8()),
            '(' => (TokenKind::LParen, start + ch.len_utf8()),
            ')' => (TokenKind::RParen, start + ch.len_utf8()),
            '{' => (TokenKind::LBrace, start + ch.len_utf8()),
            '}' => (TokenKind::RBrace, start + ch.len_utf8()),
            ';' => (TokenKind::Semicolon, start + ch.len_utf8()),
            _ if is_identifier_start(ch) => {
                let mut end = start + ch.len_utf8();
                while let Some((next_index, next_ch)) = chars.peek().copied() {
                    if is_identifier_continue(next_ch) {
                        chars.next();
                        end = next_index + next_ch.len_utf8();
                    } else {
                        break;
                    }
                }
                (keyword_kind(&input[start..end]), end)
            }
            _ if ch.is_ascii_digit() => {
                let mut end = start + ch.len_utf8();
                while let Some((next_index, next_ch)) = chars.peek().copied() {
                    if next_ch.is_ascii_digit() {
                        chars.next();
                        end = next_index + next_ch.len_utf8();
                    } else {
                        break;
                    }
                }
                (TokenKind::Number, end)
            }
            _ => {
                return Err(SrplDiagnostic::new(
                    DiagnosticPhase::Lexing,
                    Some(SourceSpan::new(start, start + ch.len_utf8())),
                    "unexpected character in SRPL procedure signature",
                ));
            }
        };

        tokens.push(Token {
            kind,
            lexeme: input[start..end].to_string(),
            span: SourceSpan::new(start, end),
        });
    }

    Ok(tokens)
}

const fn is_identifier_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

const fn is_identifier_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn keyword_kind(value: &str) -> TokenKind {
    match value.to_ascii_lowercase().as_str() {
        "procedure" => TokenKind::Procedure,
        "accepts" => TokenKind::Accepts,
        "returns" => TokenKind::Returns,
        "one" => TokenKind::One,
        "optionalone" | "optional_one" => TokenKind::OptionalOne,
        "many" => TokenKind::Many,
        "nonemptymany" | "non_empty_many" => TokenKind::NonEmptyMany,
        _ => TokenKind::Identifier,
    }
}

#[cfg(test)]
mod tests {
    use andromeda_srpl_diagnostics::{DiagnosticPhase, SourceSpan};

    use super::{TokenKind, lex};

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
    fn emits_keywords_punctuation_numbers_and_byte_spans() {
        let source = "procedure Inventory.ReserveStock accepts (ProductId i64) \
            returns Reservation one (Reserved bool) body { ensure Stock >= 1; }";

        let tokens = lex(source).expect("lexer must accept the narrow SRPL surface");

        assert_eq!(tokens[0].kind, TokenKind::Procedure);
        assert!(tokens.iter().any(|token| token.kind == TokenKind::Dot));
        assert!(tokens.iter().any(|token| token.kind == TokenKind::LParen));
        assert!(tokens.iter().any(|token| token.kind == TokenKind::RParen));
        assert!(tokens.iter().any(|token| token.kind == TokenKind::LBrace));
        assert!(tokens.iter().any(|token| token.kind == TokenKind::RBrace));
        assert!(
            tokens
                .iter()
                .any(|token| token.kind == TokenKind::GreaterEqual)
        );
        assert!(tokens.iter().any(|token| token.kind == TokenKind::Number));
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
    fn recognizes_compound_cardinality_keyword_forms() {
        let source = "optional_one optionalone non_empty_many nonemptymany optional nonempty";
        let tokens = lex(source).expect("compound cardinality keywords must lex");
        let kinds = tokens.iter().map(|token| token.kind).collect::<Vec<_>>();

        assert_eq!(
            kinds,
            vec![
                TokenKind::OptionalOne,
                TokenKind::OptionalOne,
                TokenKind::NonEmptyMany,
                TokenKind::NonEmptyMany,
                TokenKind::Identifier,
                TokenKind::Identifier,
            ]
        );
    }

    #[test]
    fn rejects_unsupported_greater_than_and_keeps_span_on_character() {
        let source = "procedure X accepts () returns R one (C bool); >";

        let diagnostic = lex(source).expect_err("bare greater-than is not an SRPL token");

        assert_eq!(diagnostic.phase, DiagnosticPhase::Lexing);
        let span = diagnostic
            .location
            .expect("lexing diagnostic must include a source span");
        assert_utf8_span(source, span);
        assert_eq!(&source[span.start..span.end], ">");
    }

    #[test]
    fn reports_utf8_safe_diagnostic_spans() {
        let source = "procedure X accepts () returns R one (C bool); \u{1f60a}";

        let diagnostic = lex(source).expect_err("emoji is not an SRPL token");

        assert_eq!(diagnostic.phase, DiagnosticPhase::Lexing);
        let span = diagnostic
            .location
            .expect("lexing diagnostic must include a source span");
        assert_utf8_span(source, span);
        assert_eq!(&source[span.start..span.end], "\u{1f60a}");
    }
}
