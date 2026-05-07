use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::identifier::validate_srpl_identifier as validate_symbol;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SrplPredicateIr {
    InputEqualsField {
        input: String,
        binding: String,
        field: String,
    },
    FieldGreaterThanOrEqualInput {
        binding: String,
        field: String,
        input: String,
    },
}

impl SrplPredicateIr {
    pub(super) fn validate(&self) -> AndromedaResult<()> {
        match self {
            Self::InputEqualsField {
                input,
                binding,
                field,
            }
            | Self::FieldGreaterThanOrEqualInput {
                binding,
                field,
                input,
            } => {
                validate_symbol(input, "SRPL predicate input")?;
                validate_symbol(binding, "SRPL predicate binding")?;
                validate_symbol(field, "SRPL predicate field")?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplAssignmentIr {
    pub field: String,
    pub value: SrplValueIr,
}

impl SrplAssignmentIr {
    pub(super) fn validate(&self) -> AndromedaResult<()> {
        validate_symbol(&self.field, "SRPL assignment field")?;
        self.value.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplEmitValueIr {
    pub column: String,
    pub value: SrplValueIr,
}

impl SrplEmitValueIr {
    pub(super) fn validate(&self) -> AndromedaResult<()> {
        validate_symbol(&self.column, "SRPL emit column")?;
        self.value.validate()
    }
}

/// Maximum nesting depth for [`SrplValueIr::BinaryArith`] expressions.
///
/// Bounded to prevent stack overflow during recursive folding.
/// Any expression tree deeper than this limit is rejected at lowering time.
pub const MAX_EXPR_DEPTH: usize = 8;

/// A typed compile-time constant literal.
///
/// The variants are **closed**. Adding a new variant is a doctrine change
/// because it expands what the optimizer is permitted to fold without
/// a runtime type check. There is deliberately no `Null` variant: null
/// values are governed by `AbsencePolicy::ExplicitOptional` and require
/// three-valued logic (TRUE / FALSE / UNKNOWN) that is outside the bounded
/// SRPL constant-folding semantics.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConstantLiteral {
    /// Boolean constant - `true` or `false`.
    Bool(bool),

    /// Signed 64-bit integer constant. Covers I8 / I16 / I32 / I64 after
    /// type-checked widening performed by the lowering pipeline.
    Int64(i64),

    /// Unsigned 64-bit integer constant. Covers U8 / U16 / U32 / U64.
    Uint64(u64),

    /// Fixed-point decimal constant stored as `(integer_part, scale)`.
    /// Invariant: `scale <= 18`. Validated at construction.
    Decimal {
        /// The unscaled integer representation.
        integer_part: i128,
        /// Number of digits to the right of the decimal point.
        scale: u8,
    },
}

impl ConstantLiteral {
    /// Stable type-tag byte for use in plan-shape fingerprints.
    /// Tags are part of the on-disk identity and must not be reordered.
    pub const fn type_tag(&self) -> u8 {
        match self {
            Self::Bool(_) => 0x01,
            Self::Int64(_) => 0x10,
            Self::Uint64(_) => 0x11,
            Self::Decimal { .. } => 0x20,
        }
    }

    /// True when this literal is type-compatible with `scalar` without
    /// silent conversion.
    pub fn is_compatible_with(&self, scalar: &andromeda_types::ScalarType) -> bool {
        use andromeda_types::ScalarType;
        matches!(
            (self, scalar),
            (Self::Bool(_), ScalarType::Bool)
                | (Self::Int64(_), ScalarType::I8)
                | (Self::Int64(_), ScalarType::I16)
                | (Self::Int64(_), ScalarType::I32)
                | (Self::Int64(_), ScalarType::I64)
                | (Self::Int64(_), ScalarType::I128)
                | (Self::Uint64(_), ScalarType::U8)
                | (Self::Uint64(_), ScalarType::U16)
                | (Self::Uint64(_), ScalarType::U32)
                | (Self::Uint64(_), ScalarType::U64)
                | (Self::Uint64(_), ScalarType::U128)
                | (Self::Decimal { .. }, ScalarType::Decimal(_))
        )
    }

    /// Validate that the decimal invariant `scale <= 18` holds.
    pub fn validate(&self) -> AndromedaResult<()> {
        if let Self::Decimal { scale, .. } = self
            && *scale > 18
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "SRPL constant decimal scale must not exceed 18",
            ));
        }
        Ok(())
    }
}

/// Bounded binary arithmetic operator for [`SrplValueIr::BinaryArith`].
///
/// The set is intentionally limited to the four basic arithmetic operations.
/// Adding a variant is a doctrine change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArithOp {
    Add,
    Subtract,
    Multiply,
    /// Division by zero is a deferred runtime error (INV-09).
    /// The constant folding pass must **not** raise a compile-time error
    /// when folding `a / 0`; instead it must leave the node in the IR.
    Divide,
}

impl ArithOp {
    /// Stable tag byte for plan-shape fingerprints.
    pub const fn as_tag(self) -> u8 {
        match self {
            Self::Add => 0x30,
            Self::Subtract => 0x31,
            Self::Multiply => 0x32,
            Self::Divide => 0x33,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SrplValueIr {
    /// Reference to a named procedure input parameter.
    Input(String),

    /// Reference to a field of a bound row-set.
    Field { binding: String, field: String },

    /// **Deprecated** - use `Constant(ConstantLiteral::Bool(_))` instead.
    /// Kept for lowering pipeline backward compatibility.
    Bool(bool),

    /// `binding.field - :input` convenience shorthand (inventory pattern).
    SubtractInput {
        binding: String,
        field: String,
        input: String,
    },

    /// A compile-time constant that has been folded or directly parsed.
    Constant(ConstantLiteral),

    /// Binary arithmetic over two sub-expressions. Both sub-trees are
    /// evaluated before the operator is applied. Maximum nesting depth
    /// is `MAX_EXPR_DEPTH`.
    BinaryArith {
        op: ArithOp,
        left: Box<SrplValueIr>,
        right: Box<SrplValueIr>,
    },
}

impl SrplValueIr {
    /// Compute the nesting depth of an expression tree.
    /// Used to enforce `MAX_EXPR_DEPTH` at lowering time.
    pub fn depth(&self) -> usize {
        match self {
            Self::BinaryArith { left, right, .. } => 1 + left.depth().max(right.depth()),
            _ => 0,
        }
    }

    /// True when this value is a compile-time constant (no runtime lookup).
    pub fn is_constant(&self) -> bool {
        match self {
            Self::Constant(_) | Self::Bool(_) => true,
            Self::BinaryArith { left, right, .. } => left.is_constant() && right.is_constant(),
            _ => false,
        }
    }

    pub(super) fn validate(&self) -> AndromedaResult<()> {
        if self.depth() > MAX_EXPR_DEPTH {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "SRPL value expression exceeds maximum nesting depth",
            ));
        }
        match self {
            Self::Input(input) => validate_symbol(input, "SRPL value input")?,
            Self::Field { binding, field } => {
                validate_symbol(binding, "SRPL value binding")?;
                validate_symbol(field, "SRPL value field")?;
            }
            Self::Bool(_) => {}
            Self::SubtractInput {
                binding,
                field,
                input,
            } => {
                validate_symbol(binding, "SRPL subtract binding")?;
                validate_symbol(field, "SRPL subtract field")?;
                validate_symbol(input, "SRPL subtract input")?;
            }
            Self::Constant(lit) => lit.validate()?,
            Self::BinaryArith { left, right, .. } => {
                left.validate()?;
                right.validate()?;
            }
        }
        Ok(())
    }
}
