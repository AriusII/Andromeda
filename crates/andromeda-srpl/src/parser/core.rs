//! Core `Parser` struct and cursor primitives.
//!
//! All other parser sub-modules add `impl Parser` blocks against the struct
//! defined here. Visibility is `pub(super)` throughout so nothing leaks past
//! the `parser` module boundary.

use crate::{DiagnosticPhase, SourceSpan, SrplDiagnostic, Token, TokenKind};

/// Recursive-descent parser for the SRPL narrow grammar.
pub(super) struct Parser {
    pub(super) tokens: Vec<Token>,
    pub(super) cursor: usize,
    pub(super) source_len: usize,
}

impl Parser {
    pub(super) fn new(tokens: Vec<Token>, source_len: usize) -> Self {
        Self {
            tokens,
            cursor: 0,
            source_len,
        }
    }

    // ---- Token-stream primitives ----------------------------------------

    pub(super) fn expect(&mut self, kind: TokenKind) -> Result<Token, SrplDiagnostic> {
        let token = self.advance().ok_or_else(|| {
            self.error_at(
                SourceSpan::new(self.source_len, self.source_len),
                format!("expected token {:?}", kind),
            )
        })?;
        if token.kind == kind {
            Ok(token)
        } else {
            Err(self.error_at(token.span, format!("expected token {:?}", kind)))
        }
    }

    pub(super) fn match_kind(&mut self, kind: TokenKind) -> Option<Token> {
        if matches!(self.peek(), Some(token) if token.kind == kind) {
            self.advance()
        } else {
            None
        }
    }

    pub(super) fn advance(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.cursor).cloned();
        if token.is_some() {
            self.cursor += 1;
        }
        token
    }

    pub(super) fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.cursor)
    }

    pub(super) fn peek_identifier_keyword(&self, keyword: &str) -> bool {
        matches!(
            self.peek(),
            Some(token)
                if token.kind == TokenKind::Identifier
                    && token.lexeme.eq_ignore_ascii_case(keyword)
        )
    }

    pub(super) fn previous_end(&self) -> usize {
        self.tokens
            .get(self.cursor.saturating_sub(1))
            .map(|token| token.span.end)
            .unwrap_or(0)
    }

    pub(super) fn expect_identifier_keyword(
        &mut self,
        keyword: &str,
    ) -> Result<Token, SrplDiagnostic> {
        let token = self.expect(TokenKind::Identifier)?;
        if token.lexeme.eq_ignore_ascii_case(keyword) {
            Ok(token)
        } else {
            Err(self.error_at(
                token.span,
                format!("expected contextual SRPL keyword {}", keyword),
            ))
        }
    }

    pub(super) fn error_at(&self, span: SourceSpan, message: impl Into<String>) -> SrplDiagnostic {
        SrplDiagnostic::new(DiagnosticPhase::Parsing, Some(span), message)
    }
}
