use andromeda_catalog::QualifiedName;
use andromeda_core::{ScalarType, TypeDescriptor};

use crate::{
    Cardinality, DiagnosticPhase, FieldAst, ProcedureAst, ResultStreamAst, SourceSpan, Spanned,
    SrplDiagnostic, Token, TokenKind, lex,
};

pub fn parse_procedure_signature(input: &str) -> Result<ProcedureAst, SrplDiagnostic> {
    let tokens = lex(input)?;
    Parser::new(tokens, input.len()).parse_procedure()
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
    source_len: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>, source_len: usize) -> Self {
        Self {
            tokens,
            cursor: 0,
            source_len,
        }
    }

    fn parse_procedure(&mut self) -> Result<ProcedureAst, SrplDiagnostic> {
        let start = self.expect(TokenKind::Procedure)?.span.start;
        let name = self.parse_qualified_name()?;
        self.expect(TokenKind::Accepts)?;
        let parameters = self.parse_field_list(false)?;
        self.expect(TokenKind::Returns)?;
        let result = self.parse_result_stream()?;
        let end = if self.match_kind(TokenKind::Semicolon).is_some() {
            self.previous_end()
        } else {
            result.span.end
        };

        if let Some(token) = self.peek() {
            return Err(self.error_at(
                token.span,
                "only one narrow procedure declaration is accepted in this SRPL slice",
            ));
        }

        Ok(ProcedureAst {
            name,
            parameters,
            results: vec![result],
            span: SourceSpan::new(start, end.min(self.source_len)),
        })
    }

    fn parse_result_stream(&mut self) -> Result<ResultStreamAst, SrplDiagnostic> {
        let name = self.expect(TokenKind::Identifier)?;
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

    fn parse_field_list(
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

    fn parse_qualified_name(&mut self) -> Result<Spanned<QualifiedName>, SrplDiagnostic> {
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

    fn parse_cardinality(&mut self) -> Result<Spanned<Cardinality>, SrplDiagnostic> {
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
            _ => return Err(self.error_at(token.span, "expected SRPL result cardinality")),
        };

        Ok(Spanned::new(cardinality, token.span))
    }

    fn parse_type(&mut self) -> Result<Spanned<TypeDescriptor>, SrplDiagnostic> {
        let token = self.expect(TokenKind::Identifier)?;
        let scalar = match token.lexeme.to_ascii_lowercase().as_str() {
            "i64" => ScalarType::I64,
            "bool" => ScalarType::Bool,
            _ => {
                return Err(self.error_at(
                    token.span,
                    "unsupported SRPL scalar type in minimal compiler slice",
                ));
            }
        };
        Ok(Spanned::new(TypeDescriptor::required(scalar), token.span))
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, SrplDiagnostic> {
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

    fn match_kind(&mut self, kind: TokenKind) -> Option<Token> {
        if matches!(self.peek(), Some(token) if token.kind == kind) {
            self.advance()
        } else {
            None
        }
    }

    fn advance(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.cursor).cloned();
        if token.is_some() {
            self.cursor += 1;
        }
        token
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.cursor)
    }

    fn previous_end(&self) -> usize {
        self.tokens
            .get(self.cursor.saturating_sub(1))
            .map(|token| token.span.end)
            .unwrap_or(0)
    }

    fn error_at(&self, span: SourceSpan, message: impl Into<String>) -> SrplDiagnostic {
        SrplDiagnostic::new(DiagnosticPhase::Parsing, Some(span), message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DiagnosticPhase;

    #[test]
    fn parses_narrow_procedure_signature_with_spans() {
        let ast = parse_procedure_signature(
            "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);",
        )
        .unwrap();

        assert_eq!(ast.name.value.as_catalog_path(), "Inventory.ReserveStock");
        assert_eq!(ast.parameters.len(), 1);
        assert_eq!(ast.results[0].cardinality.value, Cardinality::One);
        assert_eq!(ast.results[0].columns.len(), 1);
        assert!(ast.span.is_valid());
    }

    #[test]
    fn parser_rejects_unsupported_surface_with_phase_and_span() {
        let diagnostic =
            parse_procedure_signature("procedure X accepts () returns R many ();").unwrap_err();

        assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
        assert!(diagnostic.location.is_some());
        assert!(diagnostic.message.contains("at least one column"));
    }
}
