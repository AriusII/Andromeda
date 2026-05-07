use crate::{DiagnosticPhase, SourceSpan, SrplDiagnostic};

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
