//! Field, identifier, cardinality, and list helper parsers.
//!
//! These are lower-level parse helpers used by both the statement parsers
//! and the type parsers. They do not depend on any statement-level logic.

use andromeda_catalog::QualifiedName;

use crate::{
    Cardinality, DiagnosticPhase, FieldAst, ResultStreamAst, SourceSpan, Spanned, SrplDiagnostic,
    TokenKind,
};

use super::core::Parser;

impl Parser {
    /// Parse a dot-separated qualified name such as `Inventory.ReserveStock`.
    pub(super) fn parse_qualified_name(
        &mut self,
    ) -> Result<Spanned<QualifiedName>, SrplDiagnostic> {
        let first = self.expect(TokenKind::Identifier)?;
        let first_start = first.span.start;
        let mut end = first.span.end;
        let mut parts = vec![first.lexeme];

        while self.match_kind(TokenKind::Dot).is_some() {
            let part = self.expect(TokenKind::Identifier)?;
            end = part.span.end;
            parts.push(part.lexeme);
        }

        let name = QualifiedName::new(parts).map_err(|error| {
            SrplDiagnostic::new(
                DiagnosticPhase::Parsing,
                Some(SourceSpan::new(first_start, end)),
                error.to_string(),
            )
        })?;

        Ok(Spanned::new(name, SourceSpan::new(first_start, end)))
    }

    /// Parse a single identifier token into a `Spanned<String>`.
    pub(super) fn parse_identifier_spanned(&mut self) -> Result<Spanned<String>, SrplDiagnostic> {
        let token = self.expect(TokenKind::Identifier)?;
        self.reject_reserved_absence_identifier(&token.lexeme, token.span)?;
        Ok(Spanned::new(token.lexeme, token.span))
    }

    /// Parse a single numeric literal token into a `Spanned<u64>`.
    pub(super) fn parse_number_spanned(&mut self) -> Result<Spanned<u64>, SrplDiagnostic> {
        let token = self.expect(TokenKind::Number)?;
        let value = token.lexeme.parse::<u64>().map_err(|_| {
            self.error_at(
                token.span,
                "SRPL numeric literal is outside the supported range",
            )
        })?;
        Ok(Spanned::new(value, token.span))
    }

    /// Parse a `binding.field` scoped field reference.
    pub(super) fn parse_scoped_field(
        &mut self,
    ) -> Result<(Spanned<String>, Spanned<String>), SrplDiagnostic> {
        let binding = self.parse_identifier_spanned()?;
        self.expect(TokenKind::Dot)?;
        let field = self.parse_identifier_spanned()?;
        Ok((binding, field))
    }

    /// Parse a parenthesized, comma-separated list of identifiers.
    ///
    /// When `require_non_empty` is `true`, an empty list is a parse error.
    pub(super) fn parse_identifier_list(
        &mut self,
        require_non_empty: bool,
    ) -> Result<Vec<Spanned<String>>, SrplDiagnostic> {
        self.expect(TokenKind::LParen)?;
        let mut values = Vec::new();
        if self.match_kind(TokenKind::RParen).is_some() {
            if require_non_empty {
                return Err(self.error_at(
                    SourceSpan::new(self.previous_end(), self.previous_end()),
                    "SRPL identifier list must declare at least one value",
                ));
            }
            return Ok(values);
        }

        loop {
            values.push(self.parse_identifier_spanned()?);
            if self.match_kind(TokenKind::Comma).is_some() {
                continue;
            }
            self.expect(TokenKind::RParen)?;
            return Ok(values);
        }
    }

    /// Parse a result stream declaration: `name cardinality (fields)`.
    pub(super) fn parse_result_stream(&mut self) -> Result<ResultStreamAst, SrplDiagnostic> {
        let name = self.expect(TokenKind::Identifier)?;
        self.reject_reserved_absence_identifier(&name.lexeme, name.span)?;
        let cardinality = self.parse_cardinality()?;
        let columns = self.parse_field_list(true)?;
        let span = SourceSpan::new(name.span.start, self.previous_end());
        Ok(ResultStreamAst {
            name: Spanned::new(name.lexeme, name.span),
            cardinality,
            columns,
            span,
        })
    }

    /// Parse a parenthesized, comma-separated list of typed fields.
    ///
    /// When `require_non_empty` is `true`, an empty field list is a parse error.
    pub(super) fn parse_field_list(
        &mut self,
        require_non_empty: bool,
    ) -> Result<Vec<FieldAst>, SrplDiagnostic> {
        self.expect(TokenKind::LParen)?;
        let mut fields = Vec::new();
        if self.match_kind(TokenKind::RParen).is_some() {
            if require_non_empty {
                return Err(self.error_at(
                    SourceSpan::new(self.previous_end(), self.previous_end()),
                    "SRPL result stream must declare at least one column",
                ));
            }
            return Ok(fields);
        }

        loop {
            let name = self.expect(TokenKind::Identifier)?;
            self.reject_reserved_absence_identifier(&name.lexeme, name.span)?;
            let data_type = self.parse_type()?;
            fields.push(FieldAst {
                name: Spanned::new(name.lexeme, name.span),
                data_type,
                ordinal: (fields.len() as u32),
            });

            if self.match_kind(TokenKind::Comma).is_some() {
                continue;
            }
            self.expect(TokenKind::RParen)?;
            return Ok(fields);
        }
    }

    /// Parse a result cardinality token or phrase:
    /// `one`, `optional one`, `optional_one`, `many`, `nonempty many`, or
    /// `non_empty_many`.
    pub(super) fn parse_cardinality(&mut self) -> Result<Spanned<Cardinality>, SrplDiagnostic> {
        let token = self.advance().ok_or_else(|| {
            self.error_at(
                SourceSpan::new(self.source_len, self.source_len),
                "expected SRPL result cardinality",
            )
        })?;
        let cardinality = match token.kind {
            TokenKind::One => Cardinality::One,
            TokenKind::OptionalOne => Cardinality::OptionalOne,
            TokenKind::Many => Cardinality::Many,
            TokenKind::NonEmptyMany => Cardinality::NonEmptyMany,
            TokenKind::Identifier if token.lexeme.eq_ignore_ascii_case("optional") => {
                let next = self.expect(TokenKind::One).map_err(|_| {
                    self.error_at(
                        token.span,
                        "SRPL optional cardinality must be written as `optional one`",
                    )
                })?;
                return Ok(Spanned::new(
                    Cardinality::OptionalOne,
                    SourceSpan::new(token.span.start, next.span.end),
                ));
            }
            TokenKind::Identifier
                if token.lexeme.eq_ignore_ascii_case("nonempty")
                    || token.lexeme.eq_ignore_ascii_case("non_empty") =>
            {
                let next = self.expect(TokenKind::Many).map_err(|_| {
                    self.error_at(
                        token.span,
                        "SRPL nonempty cardinality must be written as `nonempty many`",
                    )
                })?;
                return Ok(Spanned::new(
                    Cardinality::NonEmptyMany,
                    SourceSpan::new(token.span.start, next.span.end),
                ));
            }
            _ => return Err(self.error_at(token.span, "expected SRPL result cardinality")),
        };

        Ok(Spanned::new(cardinality, token.span))
    }

    fn reject_reserved_absence_identifier(
        &self,
        lexeme: &str,
        span: SourceSpan,
    ) -> Result<(), SrplDiagnostic> {
        if matches!(
            lexeme.to_ascii_lowercase().as_str(),
            "null" | "nullable" | "optional" | "option" | "maybe"
        ) {
            return Err(self.error_at(
                span,
                "SRPL V0 reserves absence keywords; model absence with explicit optional cardinality and branching",
            ));
        }

        Ok(())
    }
}
