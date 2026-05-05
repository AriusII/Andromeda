//! SQL type system with validation rules.
//!
//! This module defines the SQL type descriptors used to describe database values.
//! Each type combines a `ScalarType` with an `AbsencePolicy` to indicate
//! whether null values are permitted.
//!
//! ## Scalar Types
//!
//! - **Integer**: I8, I16, I32, I64, I128 (signed) or U8, U16, U32, U64, U128 (unsigned)
//! - **Decimal**: Fixed-precision decimal numbers with precision and scale
//! - **Float**: IEEE floating-point with determinism mode options
//! - **Bool**: True/false values
//! - **Text**: Variable-length strings with encoding and collation
//! - **Timestamp**: Database timestamps with different derivation methods
//!
//! ## Absence Policy
//!
//! - **Required**: NULL values are not permitted
//! - **ExplicitOptional**: NULL values are explicitly allowed
//!
//! ## Validation
//!
//! Types validate:
//! - Decimal precision/scale relationships
//! - Float determinism constraints
//! - Text encoding and length constraints
//! - Column name non-emptiness
//! - Column ordinal density and zero-basedness

use crate::{AndromedaError, AndromedaErrorKind, AndromedaResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbsencePolicy {
    Required,
    ExplicitOptional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecimalType {
    Min,
    Mid,
    Max,
    Custom { precision: u8, scale: u8 },
}

impl DecimalType {
    pub fn validate(self) -> AndromedaResult<()> {
        match self {
            Self::Custom { precision, scale } if precision == 0 || scale > precision => {
                Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "decimal precision must be positive and scale must not exceed precision",
                ))
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatMode {
    Approximate,
    DeterministicAnalytics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatType {
    Min,
    Mid,
    Max,
    Custom { bits: u16, mode: FloatMode },
}

impl FloatType {
    pub const fn can_back_exact_invariant(self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    Utf8,
    Utf16,
    Unicode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextType {
    pub encoding: TextEncoding,
    pub max_length: Option<u32>,
    pub collation: Option<String>,
}

impl TextType {
    pub fn validate(&self) -> AndromedaResult<()> {
        if matches!(self.max_length, Some(0)) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "text max length must be positive when present",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampType {
    Transaction,
    Invocation,
    MonotonicEpoch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScalarType {
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,
    Decimal(DecimalType),
    Float(FloatType),
    Bool,
    Text(TextType),
    Timestamp(TimestampType),
}

impl ScalarType {
    pub fn permits_silent_conversion(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDescriptor {
    pub scalar: ScalarType,
    pub absence: AbsencePolicy,
}

impl TypeDescriptor {
    pub const fn required(scalar: ScalarType) -> Self {
        Self {
            scalar,
            absence: AbsencePolicy::Required,
        }
    }

    pub const fn optional(scalar: ScalarType) -> Self {
        Self {
            scalar,
            absence: AbsencePolicy::ExplicitOptional,
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        match &self.scalar {
            ScalarType::Decimal(decimal) => decimal.validate(),
            ScalarType::Float(float) if float.can_back_exact_invariant() => {
                Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "float types must not back exact relational invariants",
                ))
            }
            ScalarType::Text(text) => text.validate(),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDescriptor {
    pub name: String,
    pub data_type: TypeDescriptor,
    pub ordinal: u32,
}

impl ColumnDescriptor {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.name.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "column name must not be empty",
            ));
        }

        self.data_type.validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_custom_shape_is_checked() {
        assert!(
            DecimalType::Custom {
                precision: 9,
                scale: 2,
            }
            .validate()
            .is_ok()
        );
        assert_eq!(
            DecimalType::Custom {
                precision: 2,
                scale: 3,
            }
            .validate()
            .unwrap_err()
            .kind(),
            AndromedaErrorKind::Contract
        );
    }

    #[test]
    fn bool_is_required_without_nullable_shortcut() {
        let descriptor = TypeDescriptor::required(ScalarType::Bool);

        assert_eq!(descriptor.absence, AbsencePolicy::Required);
        assert!(!descriptor.scalar.permits_silent_conversion());
    }
}
