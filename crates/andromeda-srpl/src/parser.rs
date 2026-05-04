use andromeda_catalog::QualifiedName;
use andromeda_core::{
    DecimalType, ScalarType, TextEncoding, TextType, TimestampType, TypeDescriptor,
};

use crate::{
    lex, BusinessOperationAst, BusinessOperationKindAst, Cardinality, DiagnosticPhase, FieldAst,
    ProcedureAst, ProcedureBodyAst, ResultStreamAst, SourceSpan, Spanned, SrplDiagnostic, Token,
    TokenKind, MAX_SRPL_BODY_OPERATIONS,
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

    fn parse_number_spanned(&mut self) -> Result<Spanned<u64>, SrplDiagnostic> {
        let token = self.expect(TokenKind::Number)?;
        let value = token.lexeme.parse::<u64>().map_err(|_| {
            self.error_at(
                token.span,
                "SRPL numeric literal is outside the supported range",
            )
        })?;
        Ok(Spanned::new(value, token.span))
    }

    fn parse_scoped_field(&mut self) -> Result<(Spanned<String>, Spanned<String>), SrplDiagnostic> {
        let binding = self.parse_identifier_spanned()?;
        self.expect(TokenKind::Dot)?;
        let field = self.parse_identifier_spanned()?;
        Ok((binding, field))
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
        let head_span = token.span;
        let lowered = token.lexeme.to_ascii_lowercase();

        // V0 absence policy: SRPL fields are unconditionally Required.
        // Reject any explicit nullability syntax until a deliberate decision
        // record introduces it. Catch the keywords up front so the diagnostic
        // is precise rather than a generic "unsupported scalar type".
        if matches!(
            lowered.as_str(),
            "null" | "nullable" | "optional" | "option" | "maybe"
        ) {
            return Err(self.error_at(
                head_span,
                "SRPL V0 forbids nullable values; declare a Required scalar type",
            ));
        }

        let (scalar, end) = match lowered.as_str() {
            // Signed integer family.
            "i8" => (ScalarType::I8, head_span.end),
            "i16" => (ScalarType::I16, head_span.end),
            "i32" => (ScalarType::I32, head_span.end),
            "i64" => (ScalarType::I64, head_span.end),
            "i128" => (ScalarType::I128, head_span.end),
            // Unsigned integer family.
            "u8" => (ScalarType::U8, head_span.end),
            "u16" => (ScalarType::U16, head_span.end),
            "u32" => (ScalarType::U32, head_span.end),
            "u64" => (ScalarType::U64, head_span.end),
            "u128" => (ScalarType::U128, head_span.end),
            // Boolean.
            "bool" => (ScalarType::Bool, head_span.end),
            // Text: `text` for unbounded UTF-8 or `text(max_length)` for a
            // bounded UTF-8 column. Encoding/collation are intentionally not
            // surfaced in the grammar yet to keep V0 deterministic.
            "text" => self.parse_text_type(head_span)?,
            // Decimal requires explicit precision and scale; never default
            // to ambiguous shapes.
            "decimal" => self.parse_decimal_type(head_span)?,
            // Timestamp requires an explicit derivation mode, mirroring the
            // core TimestampType variants.
            "timestamp" => self.parse_timestamp_type(head_span)?,
            // Float family is intentionally not exposed in SRPL: the core
            // type system marks float as unsuitable for exact relational
            // invariants (see `FloatType::can_back_exact_invariant`).
            "f32" | "f64" | "float" | "double" => {
                return Err(self.error_at(
                    head_span,
                    "float scalar types are not permitted in SRPL contracts",
                ));
            }
            _ => {
                return Err(self.error_at(
                    head_span,
                    "unsupported SRPL scalar type; expected one of \
                     i8|i16|i32|i64|i128|u8|u16|u32|u64|u128|bool|text|decimal|timestamp",
                ));
            }
        };

        let descriptor = TypeDescriptor::required(scalar);
        // Validate the descriptor eagerly so that contract-level invariants
        // (e.g. decimal precision/scale, text length) are caught at parse
        // time rather than during binding. Map the resulting error back to
        // a parsing diagnostic with the original span.
        descriptor
            .validate()
            .map_err(|err| self.error_at(SourceSpan::new(head_span.start, end), err.message()))?;

        Ok(Spanned::new(
            descriptor,
            SourceSpan::new(head_span.start, end),
        ))
    }

    fn parse_text_type(
        &mut self,
        head_span: SourceSpan,
    ) -> Result<(ScalarType, usize), SrplDiagnostic> {
        let mut end = head_span.end;
        let max_length = if self.match_kind(TokenKind::LParen).is_some() {
            let length_token = self.parse_number_spanned()?;
            let length = u32::try_from(length_token.value).map_err(|_| {
                self.error_at(
                    length_token.span,
                    "SRPL text max length is outside the supported range",
                )
            })?;
            let close = self.expect(TokenKind::RParen)?;
            end = close.span.end;
            Some(length)
        } else {
            None
        };

        Ok((
            ScalarType::Text(TextType {
                encoding: TextEncoding::Utf8,
                max_length,
                collation: None,
            }),
            end,
        ))
    }

    fn parse_decimal_type(
        &mut self,
        head_span: SourceSpan,
    ) -> Result<(ScalarType, usize), SrplDiagnostic> {
        // Decimal in SRPL must be explicit: `decimal(precision, scale)`.
        // Defaulting would silently couple SRPL contracts to a particular
        // catalog policy.
        self.expect(TokenKind::LParen).map_err(|_| {
            self.error_at(
                head_span,
                "SRPL decimal requires explicit precision and scale: decimal(p, s)",
            )
        })?;
        let precision_token = self.parse_number_spanned()?;
        self.expect(TokenKind::Comma)?;
        let scale_token = self.parse_number_spanned()?;
        let close = self.expect(TokenKind::RParen)?;

        let precision = u8::try_from(precision_token.value).map_err(|_| {
            self.error_at(
                precision_token.span,
                "SRPL decimal precision must fit in a u8",
            )
        })?;
        let scale = u8::try_from(scale_token.value)
            .map_err(|_| self.error_at(scale_token.span, "SRPL decimal scale must fit in a u8"))?;

        Ok((
            ScalarType::Decimal(DecimalType::Custom { precision, scale }),
            close.span.end,
        ))
    }

    fn parse_timestamp_type(
        &mut self,
        head_span: SourceSpan,
    ) -> Result<(ScalarType, usize), SrplDiagnostic> {
        // Timestamp derivation mode is required; never default silently.
        self.expect(TokenKind::LParen).map_err(|_| {
            self.error_at(
                head_span,
                "SRPL timestamp requires an explicit derivation mode: \
                 timestamp(transaction|invocation|monotonic_epoch)",
            )
        })?;
        let mode_token = self.expect(TokenKind::Identifier)?;
        let mode = match mode_token.lexeme.to_ascii_lowercase().as_str() {
            "transaction" => TimestampType::Transaction,
            "invocation" => TimestampType::Invocation,
            "monotonic_epoch" | "monotonicepoch" => TimestampType::MonotonicEpoch,
            _ => {
                return Err(self.error_at(
                    mode_token.span,
                    "SRPL timestamp mode must be transaction, invocation, or monotonic_epoch",
                ));
            }
        };
        let close = self.expect(TokenKind::RParen)?;
        Ok((ScalarType::Timestamp(mode), close.span.end))
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
    fn parses_begin_end_inventory_operations() {
        let ast = parse_procedure_signature(
            "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) begin ensure Inventory.ProductStock Stock where ProductId = Stock.ProductId and Stock.AvailableQuantity >= Quantity else fail InsufficientStock; update Inventory.ProductStock set AvailableQuantity = Stock.AvailableQuantity - Quantity where ProductId = Stock.ProductId affected rows 1; return Reservation (Reserved); end;",
        )
            .unwrap();

        assert_eq!(ast.body.operations.len(), 3);
        assert!(matches!(
            &ast.body.operations[0].kind,
            BusinessOperationKindAst::Ensure { failure_code, .. }
                if failure_code.value.as_str() == "InsufficientStock"
        ));
        assert!(matches!(
            &ast.body.operations[1].kind,
            BusinessOperationKindAst::UpdateSet {
                affected_rows_exact,
                ..
            } if affected_rows_exact.value == 1
        ));
        assert!(matches!(
            &ast.body.operations[2].kind,
            BusinessOperationKindAst::Return { stream, .. }
                if stream.value.as_str() == "Reservation"
        ));
    }

    #[test]
    fn parser_rejects_unknown_body_operation_before_lowering() {
        let diagnostic = parse_procedure_signature(
            "procedure X accepts () returns R one (C bool) body { compute C; }",
        )
        .unwrap_err();

        assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
        assert!(diagnostic.location.is_some());
        assert!(diagnostic
            .message
            .contains("unsupported SRPL body operation"));
    }

    // ---- Type grammar expansion (Wave 7 Agent 1) ----

    fn parse_field_types(source: &str) -> Vec<TypeDescriptor> {
        let ast = parse_procedure_signature(source).expect("source must parse");
        ast.parameters
            .into_iter()
            .map(|f| f.data_type.value)
            .collect()
    }

    #[test]
    fn parses_full_signed_and_unsigned_integer_families() {
        let descriptors = parse_field_types(
            "procedure P accepts (\
                A i8, B i16, C i32, D i64, E i128, \
                F u8, G u16, H u32, I u64, J u128\
             ) returns R one (X bool);",
        );
        let scalars: Vec<_> = descriptors.iter().map(|d| d.scalar.clone()).collect();
        assert_eq!(
            scalars,
            vec![
                ScalarType::I8,
                ScalarType::I16,
                ScalarType::I32,
                ScalarType::I64,
                ScalarType::I128,
                ScalarType::U8,
                ScalarType::U16,
                ScalarType::U32,
                ScalarType::U64,
                ScalarType::U128,
            ]
        );
        assert!(descriptors
            .iter()
            .all(|d| d.absence == andromeda_core::AbsencePolicy::Required));
    }

    #[test]
    fn parses_text_with_and_without_max_length() {
        let descriptors = parse_field_types(
            "procedure P accepts (Name text, Sku text(64)) returns R one (X bool);",
        );
        match &descriptors[0].scalar {
            ScalarType::Text(t) => {
                assert_eq!(t.encoding, TextEncoding::Utf8);
                assert!(t.max_length.is_none());
                assert!(t.collation.is_none());
            }
            other => panic!("expected text, got {:?}", other),
        }
        match &descriptors[1].scalar {
            ScalarType::Text(t) => assert_eq!(t.max_length, Some(64)),
            other => panic!("expected text(64), got {:?}", other),
        }
    }

    #[test]
    fn parses_decimal_with_explicit_precision_and_scale() {
        let descriptors =
            parse_field_types("procedure P accepts (Price decimal(18, 4)) returns R one (X bool);");
        match &descriptors[0].scalar {
            ScalarType::Decimal(DecimalType::Custom { precision, scale }) => {
                assert_eq!(*precision, 18);
                assert_eq!(*scale, 4);
            }
            other => panic!("expected decimal(18, 4), got {:?}", other),
        }
    }

    #[test]
    fn parser_rejects_decimal_without_precision_and_scale() {
        let diagnostic =
            parse_procedure_signature("procedure P accepts (X decimal) returns R one (Y bool);")
                .unwrap_err();
        assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
        assert!(diagnostic
            .message
            .contains("decimal requires explicit precision and scale"));
    }

    #[test]
    fn parser_rejects_decimal_with_scale_exceeding_precision() {
        let diagnostic = parse_procedure_signature(
            "procedure P accepts (X decimal(2, 5)) returns R one (Y bool);",
        )
        .unwrap_err();
        assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
        assert!(diagnostic.message.contains("scale"));
    }

    #[test]
    fn parses_timestamp_modes() {
        let descriptors = parse_field_types(
            "procedure P accepts (\
                T1 timestamp(transaction), \
                T2 timestamp(invocation), \
                T3 timestamp(monotonic_epoch)\
             ) returns R one (X bool);",
        );
        let scalars: Vec<_> = descriptors.iter().map(|d| d.scalar.clone()).collect();
        assert_eq!(
            scalars,
            vec![
                ScalarType::Timestamp(TimestampType::Transaction),
                ScalarType::Timestamp(TimestampType::Invocation),
                ScalarType::Timestamp(TimestampType::MonotonicEpoch),
            ]
        );
    }

    #[test]
    fn parser_rejects_timestamp_without_mode() {
        let diagnostic =
            parse_procedure_signature("procedure P accepts (T timestamp) returns R one (X bool);")
                .unwrap_err();
        assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
        assert!(diagnostic
            .message
            .contains("timestamp requires an explicit derivation mode"));
    }

    #[test]
    fn parser_rejects_unknown_timestamp_mode() {
        let diagnostic = parse_procedure_signature(
            "procedure P accepts (T timestamp(walltime)) returns R one (X bool);",
        )
        .unwrap_err();
        assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
        assert!(diagnostic.message.contains("monotonic_epoch"));
    }

    #[test]
    fn parser_rejects_float_family_explicitly() {
        for ty in ["f32", "f64", "float", "double"] {
            let source = format!(
                "procedure P accepts (X {ty}) returns R one (Y bool);",
                ty = ty
            );
            let diagnostic = parse_procedure_signature(&source).unwrap_err();
            assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
            assert!(
                diagnostic.message.contains("float scalar types"),
                "expected float rejection for {}, got {:?}",
                ty,
                diagnostic.message
            );
        }
    }

    #[test]
    fn parser_rejects_nullability_keywords_with_explicit_diagnostic() {
        for keyword in ["null", "nullable", "optional", "option", "maybe"] {
            let source = format!(
                "procedure P accepts (X {kw}) returns R one (Y bool);",
                kw = keyword
            );
            let diagnostic = parse_procedure_signature(&source).unwrap_err();
            assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
            assert!(
                diagnostic.message.contains("V0 forbids nullable values"),
                "expected nullability rejection for {}, got {:?}",
                keyword,
                diagnostic.message
            );
        }
    }

    #[test]
    fn parser_rejects_unknown_scalar_with_helpful_listing() {
        let diagnostic =
            parse_procedure_signature("procedure P accepts (X uuid) returns R one (Y bool);")
                .unwrap_err();
        assert_eq!(diagnostic.phase, DiagnosticPhase::Parsing);
        assert!(diagnostic.message.contains("unsupported SRPL scalar type"));
        assert!(diagnostic.message.contains("decimal"));
    }

    #[test]
    fn deterministic_lowering_preserves_explicit_scalar_types_in_columns() {
        // Same source parsed twice must yield byte-equal column descriptors,
        // including text bounds and decimal precision/scale.
        let source = "procedure Inventory.Detail accepts (Sku text(32), Price decimal(12, 2)) \
                      returns Detail one (Sku text(32), Price decimal(12, 2), TouchedAt timestamp(transaction));";
        let a = parse_procedure_signature(source).unwrap();
        let b = parse_procedure_signature(source).unwrap();

        let to_types = |ast: &ProcedureAst| -> Vec<TypeDescriptor> {
            ast.parameters
                .iter()
                .map(|f| f.data_type.value.clone())
                .chain(
                    ast.results[0]
                        .columns
                        .iter()
                        .map(|f| f.data_type.value.clone()),
                )
                .collect()
        };

        assert_eq!(to_types(&a), to_types(&b));
    }
}
