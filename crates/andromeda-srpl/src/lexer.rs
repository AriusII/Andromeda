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
    LParen,
    RParen,
    LBrace,
    RBrace,
    Semicolon,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub lexeme: String,
    pub span: SourceSpan,
}

pub fn lex(input: &str) -> Result<Vec<Token>, SrplDiagnostic> {
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        let ch = bytes[index] as char;
        if ch.is_ascii_whitespace() {
            index += 1;
            continue;
        }

        let start = index;
        let kind = match ch {
            '.' => {
                index += 1;
                TokenKind::Dot
            }
            ',' => {
                index += 1;
                TokenKind::Comma
            }
            '(' => {
                index += 1;
                TokenKind::LParen
            }
            ')' => {
                index += 1;
                TokenKind::RParen
            }
            '{' => {
                index += 1;
                TokenKind::LBrace
            }
            '}' => {
                index += 1;
                TokenKind::RBrace
            }
            ';' => {
                index += 1;
                TokenKind::Semicolon
            }
            _ if is_identifier_start(ch) => {
                index += 1;
                while index < bytes.len() && is_identifier_continue(bytes[index] as char) {
                    index += 1;
                }
                keyword_kind(&input[start..index])
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
            lexeme: input[start..index].to_string(),
            span: SourceSpan::new(start, index),
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
