//! Procedure and statement grammar for the bounded SRPL parser.

use crate::{
    BusinessOperationAst, BusinessOperationKindAst, MAX_SRPL_BODY_OPERATIONS, ProcedureAst,
    ProcedureBodyAst, SourceSpan, SrplDiagnostic, TokenKind,
};

use super::core::Parser;

impl Parser {
    pub(super) fn parse_procedure(&mut self) -> Result<ProcedureAst, SrplDiagnostic> {
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
        } else if self.peek_identifier_keyword("begin") {
            let body = self.parse_begin_end_body()?;
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
                BusinessOperationKindAst::Update {
                    target,
                    mutation,
                    affected_rows_exact: None,
                }
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

    fn parse_begin_end_body(&mut self) -> Result<ProcedureBodyAst, SrplDiagnostic> {
        let start = self.expect_identifier_keyword("begin")?.span.start;
        let mut operations = Vec::new();

        loop {
            if self.peek_identifier_keyword("end") {
                self.advance();
                break;
            }

            let Some(token) = self.peek() else {
                return Err(self.error_at(
                    SourceSpan::new(self.source_len, self.source_len),
                    "expected SRPL begin/end operation or end",
                ));
            };

            if operations.len() >= MAX_SRPL_BODY_OPERATIONS {
                return Err(self.error_at(
                    token.span,
                    "SRPL procedure body exceeds the bounded operation limit",
                ));
            }

            let operation = self.parse_begin_end_operation(operations.len() as u32)?;
            operations.push(operation);
            if self.match_kind(TokenKind::Semicolon).is_none()
                && !self.peek_identifier_keyword("end")
            {
                let span = self
                    .peek()
                    .map(|token| token.span)
                    .unwrap_or_else(|| SourceSpan::new(self.source_len, self.source_len));
                return Err(self.error_at(span, "expected token Semicolon"));
            }
        }

        Ok(ProcedureBodyAst {
            operations,
            span: SourceSpan::new(start, self.previous_end()),
        })
    }

    fn parse_begin_end_operation(
        &mut self,
        ordinal: u32,
    ) -> Result<BusinessOperationAst, SrplDiagnostic> {
        let operator = self.expect(TokenKind::Identifier)?;
        let operation_start = operator.span.start;
        let kind = match operator.lexeme.to_ascii_lowercase().as_str() {
            "ensure" => self.parse_ensure_operation()?,
            "update" => self.parse_update_set_operation()?,
            "return" => {
                let stream = self.parse_identifier_spanned()?;
                let values = self.parse_identifier_list(true)?;
                BusinessOperationKindAst::Return { stream, values }
            }
            _ => {
                return Err(self.error_at(
                    operator.span,
                    "unsupported SRPL begin/end operation in bounded compiler slice",
                ));
            }
        };

        Ok(BusinessOperationAst {
            ordinal,
            kind,
            span: SourceSpan::new(operation_start, self.previous_end()),
        })
    }

    fn parse_ensure_operation(&mut self) -> Result<BusinessOperationKindAst, SrplDiagnostic> {
        let source = self.parse_qualified_name()?;
        let binding = self.parse_identifier_spanned()?;
        self.expect_identifier_keyword("where")?;
        let lookup_input = self.parse_identifier_spanned()?;
        self.expect(TokenKind::Equal)?;
        let (lookup_binding, lookup_field) = self.parse_scoped_field()?;
        if lookup_binding.value != binding.value {
            return Err(self.error_at(
                lookup_binding.span,
                "SRPL ensure lookup binding must match the ensure binding",
            ));
        }
        self.expect_identifier_keyword("and")?;
        let (quantity_binding, quantity_field) = self.parse_scoped_field()?;
        if quantity_binding.value != binding.value {
            return Err(self.error_at(
                quantity_binding.span,
                "SRPL ensure quantity binding must match the ensure binding",
            ));
        }
        self.expect(TokenKind::GreaterEqual)?;
        let quantity_input = self.parse_identifier_spanned()?;
        self.expect_identifier_keyword("else")?;
        self.expect_identifier_keyword("fail")?;
        let failure_code = self.parse_identifier_spanned()?;

        Ok(BusinessOperationKindAst::Ensure {
            source,
            binding,
            lookup_input,
            lookup_field,
            quantity_field,
            quantity_input,
            failure_code,
        })
    }

    fn parse_update_set_operation(&mut self) -> Result<BusinessOperationKindAst, SrplDiagnostic> {
        let target = self.parse_qualified_name()?;
        self.expect_identifier_keyword("set")?;
        let field = self.parse_identifier_spanned()?;
        self.expect(TokenKind::Equal)?;
        let (value_binding, value_field) = self.parse_scoped_field()?;
        self.expect(TokenKind::Minus)?;
        let value_input = self.parse_identifier_spanned()?;
        self.expect_identifier_keyword("where")?;
        let where_input = self.parse_identifier_spanned()?;
        self.expect(TokenKind::Equal)?;
        let (where_binding, where_field) = self.parse_scoped_field()?;
        self.expect_identifier_keyword("affected")?;
        self.expect_identifier_keyword("rows")?;
        let affected_rows_exact = self.parse_number_spanned()?;

        Ok(BusinessOperationKindAst::UpdateSet {
            target,
            field,
            value_binding,
            value_field,
            value_input,
            where_input,
            where_binding,
            where_field,
            affected_rows_exact,
        })
    }
}
