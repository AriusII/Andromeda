//! Andromeda type descriptors with validation rules.
//!
//! This module defines the descriptors used for Procedure contracts and
//! ResultStream shapes. Each type combines a `ScalarType` with an
//! `AbsencePolicy` to make absence explicit.
//!
//! ## Scalar Types
//!
//! - **Integer**: I8, I16, I32, I64, I128 (signed) or U8, U16, U32, U64, U128 (unsigned)
//! - **Decimal**: Fixed-precision decimal numbers with precision and scale
//! - **Float**: IEEE floating-point with determinism mode options
//! - **Bool**: True/false values
//! - **Text**: Variable-length strings with encoding and collation
//! - **Timestamp**: Engine timestamps with different derivation methods
//!
//! ## Absence Policy
//!
//! - **Required**: absence is not permitted
//! - **ExplicitOptional**: absence is explicitly allowed
//!
//! ## Validation
//!
//! Types validate:
//! - Decimal precision/scale relationships
//! - Float determinism constraints
//! - Text encoding, length, and collation constraints
//! - Per-column name non-emptiness
//!
//! Callers that own a column collection validate cross-column rules such as
//! ordinal density and name uniqueness.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

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
            },
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
    pub fn validate(self) -> AndromedaResult<()> {
        match self {
            Self::Custom { bits, mode } => {
                if !matches!(bits, 16 | 32 | 64 | 128) {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Contract,
                        "custom float bits must be one of 16, 32, 64, or 128",
                    ));
                }

                if mode != FloatMode::DeterministicAnalytics {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Contract,
                        "custom float mode must be deterministic",
                    ));
                }

                Ok(())
            },
            _ => Ok(()),
        }
    }

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

        if let Some(collation) = &self.collation
            && collation.trim().is_empty()
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "text collation must not be empty when present",
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
            ScalarType::Float(float) => {
                float.validate()?;

                if float.can_back_exact_invariant() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Contract,
                        "float types must not back exact relational invariants",
                    ));
                }

                Ok(())
            },
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
        assert!(matches!(
            DecimalType::Custom {
                precision: 2,
                scale: 3,
            }
            .validate(),
            Err(error) if error.kind() == AndromedaErrorKind::Contract
        ));
    }

    #[test]
    fn decimal_custom_rejects_zero_precision_through_type_descriptor() {
        assert!(matches!(
            TypeDescriptor::required(ScalarType::Decimal(DecimalType::Custom {
                precision: 0,
                scale: 0,
            }))
            .validate(),
            Err(error) if error.kind() == AndromedaErrorKind::Contract
        ));
    }

    #[test]
    fn float_custom_deterministic_ieee_width_is_checked() {
        assert!(
            FloatType::Custom {
                bits: 64,
                mode: FloatMode::DeterministicAnalytics,
            }
            .validate()
            .is_ok()
        );
        assert!(
            TypeDescriptor::required(ScalarType::Float(FloatType::Custom {
                bits: 32,
                mode: FloatMode::DeterministicAnalytics,
            }))
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn float_custom_rejects_unbounded_or_approximate_shapes() {
        assert!(matches!(
            FloatType::Custom {
                bits: 0,
                mode: FloatMode::DeterministicAnalytics,
            }
            .validate(),
            Err(error) if error.kind() == AndromedaErrorKind::Contract
        ));
        assert!(matches!(
            FloatType::Custom {
                bits: 24,
                mode: FloatMode::DeterministicAnalytics,
            }
            .validate(),
            Err(error) if error.kind() == AndromedaErrorKind::Contract
        ));
        assert!(matches!(
            FloatType::Custom {
                bits: 64,
                mode: FloatMode::Approximate,
            }
            .validate(),
            Err(error) if error.kind() == AndromedaErrorKind::Contract
        ));
        assert!(matches!(
            TypeDescriptor::required(ScalarType::Float(FloatType::Custom {
                bits: 256,
                mode: FloatMode::Approximate,
            }))
            .validate(),
            Err(error) if error.kind() == AndromedaErrorKind::Contract
        ));
    }

    #[test]
    fn text_bounds_and_collation_are_checked() {
        assert!(
            TypeDescriptor::required(ScalarType::Text(TextType {
                encoding: TextEncoding::Utf8,
                max_length: Some(128),
                collation: Some("unicode:case-sensitive".to_string()),
            }))
            .validate()
            .is_ok()
        );

        assert!(matches!(
            TypeDescriptor::required(ScalarType::Text(TextType {
                encoding: TextEncoding::Utf8,
                max_length: Some(0),
                collation: None,
            }))
            .validate(),
            Err(error) if error.kind() == AndromedaErrorKind::Contract
        ));

        assert!(matches!(
            TypeDescriptor::required(ScalarType::Text(TextType {
                encoding: TextEncoding::Utf16,
                max_length: Some(64),
                collation: Some("  ".to_string()),
            }))
            .validate(),
            Err(error) if error.kind() == AndromedaErrorKind::Contract
        ));
    }

    #[test]
    fn absence_policy_is_explicit_contract_shape() {
        let required = TypeDescriptor::required(ScalarType::I64);
        let optional = TypeDescriptor::optional(ScalarType::I64);

        assert_eq!(required.absence, AbsencePolicy::Required);
        assert_eq!(optional.absence, AbsencePolicy::ExplicitOptional);
        assert_eq!(required.scalar, optional.scalar);
        assert!(required.validate().is_ok());
        assert!(optional.validate().is_ok());
        assert!(!required.scalar.permits_silent_conversion());
        assert!(!optional.scalar.permits_silent_conversion());
    }

    #[test]
    fn bool_is_required_without_implicit_absence_shortcut() {
        let descriptor = TypeDescriptor::required(ScalarType::Bool);

        assert_eq!(descriptor.absence, AbsencePolicy::Required);
        assert!(!descriptor.scalar.permits_silent_conversion());
    }
}
