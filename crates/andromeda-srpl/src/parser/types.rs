//! Type grammar: `parse_type` and the concrete scalar sub-parsers.
//!
//! Handles the scalar type surface exposed by SRPL V0:
//! integers, `bool`, `text[(max)]`, `decimal(p, s)`, and
//! `timestamp(mode)`. Float and nullable keywords are rejected with
//! explicit diagnostics.

use andromeda_core::{
    DecimalType, ScalarType, TextEncoding, TextType, TimestampType, TypeDescriptor,
};

use crate::{SourceSpan, Spanned, SrplDiagnostic, TokenKind};

use super::core::Parser;

impl Parser {
    /// Parse a single SRPL field type, returning a spanned `TypeDescriptor`.
    pub(super) fn parse_type(&mut self) -> Result<Spanned<TypeDescriptor>, SrplDiagnostic> {
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

    pub(super) fn parse_text_type(
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

    pub(super) fn parse_decimal_type(
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

    pub(super) fn parse_timestamp_type(
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
}
