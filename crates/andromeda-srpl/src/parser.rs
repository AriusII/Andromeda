use andromeda_catalog::QualifiedName;
use andromeda_core::{ScalarType, TypeDescriptor};

use crate::{
    BusinessOperationAst, BusinessOperationKindAst, Cardinality, DiagnosticPhase, FieldAst,
    MAX_SRPL_BODY_OPERATIONS, ProcedureAst, ProcedureBodyAst, ResultStreamAst, SourceSpan, Spanned,
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
        let mut end = if self.match_kind(TokenKind::Semicolon).is_some() {
            self.previous_end()
        } else {
            result.span.end
        };
        let body = if self.peek_identifier_keyword("body") {
            let body = self.parse_body()?;
            end = body.span.end;
            if self.match_kind(TokenKind::Semicolon).is_some() {
                end = self.previous_end();
            }
            body
        } else {
            ProcedureBodyAst {
                operations: Vec::new(),
                span: SourceSpan::new(end, end),
            }
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
            body,
            span: SourceSpan::new(start, end.min(self.source_len)),
        })
    }

    fn parse_body(&mut self) -> Result<ProcedureBodyAst, SrplDiagnostic> {
        let start = self.expect_identifier_keyword("body")?.span.start;
        self.expect(TokenKind::LBrace)?;

        let mut operations = Vec::new();
        loop {
            if self.match_kind(TokenKind::RBrace).is_some() {
                break;
            }

            let Some(token) = self.peek() else {
                return Err(self.error_at(
                    SourceSpan::new(self.source_len, self.source_len),
                    "expected SRPL body operation or closing brace",
                ));
            };

            if operations.len() >= MAX_SRPL_BODY_OPERATIONS {
                return Err(self.error_at(
                    token.span,
                    "SRPL procedure body exceeds the bounded operation limit",
                ));
            }

            let operation = self.parse_body_operation(operations.len() as u32)?;
            operations.push(operation);
            self.expect(TokenKind::Semicolon)?;
        }

        Ok(ProcedureBodyAst {
            operations,
            span: SourceSpan::new(start, self.previous_end()),
        })
    }

    fn parse_body_operation(
        &mut self,
        ordinal: u32,
    ) -> Result<BusinessOperationAst, SrplDiagnostic> {
        let operator = self.expect(TokenKind::Identifier)?;
        let operation_start = operator.span.start;
        let kind = match operator.lexeme.to_ascii_lowercase().as_str() {
            "read" => {
                let source = self.parse_qualified_name()?;
                let binding = self.parse_identifier_spanned()?;
                let cardinality = self.parse_cardinality()?;
                BusinessOperationKindAst::Read {
                    source,
                    binding,
                    cardinality,
                }
            }
            "assert" => {
                let predicate = self.parse_identifier_spanned()?;
                let failure_code = self.parse_identifier_spanned()?;
                BusinessOperationKindAst::Assert {
                    predicate,
                    failure_code,
                }
            }
            "update" => {
                let target = self.parse_qualified_name()?;
                let mutation = self.parse_identifier_spanned()?;
                BusinessOperationKindAst::Update { target, mutation }
            }
            "emit" => {
                let stream = self.parse_identifier_spanned()?;
                let values = self.parse_identifier_list(true)?;
                BusinessOperationKindAst::Emit { stream, values }
            }
            "raise" => {
                let code = self.parse_identifier_spanned()?;
                BusinessOperationKindAst::Raise { code }
            }
            _ => {
                return Err(self.error_at(
                    operator.span,
                    "unsupported SRPL body operation in bounded compiler slice",
                ));
            }
        };

        Ok(BusinessOperationAst {
            ordinal,
            kind,
            span: SourceSpan::new(operation_start, self.previous_end()),
        })
    }

    fn parse_identifier_list(
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

    fn parse_identifier_spanned(&mut self) -> Result<Spanned<String>, SrplDiagnostic> {
        let token = self.expect(TokenKind::Identifier)?;
        Ok(Spanned::new(token.lexeme, token.span))
    }

    fn expect_identifier_keyword(&mut self, keyword: &str) -> Result<Token, SrplDiagnostic> {
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

    fn peek_identifier_keyword(&self, keyword: &str) -> bool {
        matches!(
            self.peek(),
            Some(token)
                if token.kind == TokenKind::Identifier
                    && token.lexeme.eq_ignore_ascii_case(keyword)
        )
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
        assert!(ast.body.operations.is_empty());
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

    #[test]
    fn parses_bounded_body_operations_in_source_order() {
        let ast = parse_procedure_signature(
            "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool) body { read Inventory.ProductStock Stock one; assert Quantity InsufficientStock; update Inventory.ProductStock AvailableQuantity; emit Reservation (Reserved); }",
        )
        .unwrap();

        assert_eq!(ast.body.operations.len(), 4);
        assert_eq!(ast.body.operations[0].ordinal, 0);
        assert_eq!(ast.body.operations[3].ordinal, 3);
        assert!(matches!(
            &ast.body.operations[0].kind,
            BusinessOperationKindAst::Read { .. }
        ));
        assert!(matches!(
            &ast.body.operations[3].kind,
            BusinessOperationKindAst::Emit { .. }
        ));
        assert!(ast.body.span.is_valid());
    }

    #[test]
    fn parser_rejects_unknown_body_operation_before_lowering() {
        let diagnostic = parse_procedure_signature(
            "procedure X accepts () returns R one (C bool) body { compute C; }",
        )
        .unwrap_err();

        assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
        assert!(diagnostic.location.is_some());
        assert!(
            diagnostic
                .message
                .contains("unsupported SRPL body operation")
        );
    }
}
