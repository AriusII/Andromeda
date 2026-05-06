use andromeda_catalog::{
    CatalogObjectRef, CompatibilityPolicy, MultiResultPolicy, ObjectKind, ProcedureContractRef,
    ProcedureErrorPolicy, ProtocolLayoutRef, QualifiedName, ResultMetadataPolicy, StatsVersion,
    TransactionPolicy,
};
use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
    ColumnDescriptor, ContractHash, ProcedureId,
};

use crate::Cardinality;

pub const MAX_SRPL_BODY_OPERATIONS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplProcedureIr {
    pub name: QualifiedName,
    pub inputs: Vec<ColumnDescriptor>,
    pub result_streams: Vec<SrplResultStreamIr>,
    pub body: SrplProcedureBodyIr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplResultStreamIr {
    pub name: String,
    pub cardinality: Cardinality,
    pub columns: Vec<ColumnDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplProcedureBodyIr {
    pub operations: Vec<SrplBusinessOperationIr>,
}

impl SrplProcedureBodyIr {
    pub fn empty() -> Self {
        Self {
            operations: Vec::new(),
        }
    }

    pub fn validate_bounded(&self) -> AndromedaResult<()> {
        if self.operations.len() > MAX_SRPL_BODY_OPERATIONS {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "SRPL procedure body exceeds the bounded operation limit",
            ));
        }

        for (expected_ordinal, operation) in self.operations.iter().enumerate() {
            if operation.ordinal != expected_ordinal as u32 {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Srpl,
                    "SRPL body operation ordinals must be dense and zero-based",
                ));
            }
            operation.kind.validate()?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplBusinessOperationIr {
    pub ordinal: u32,
    pub kind: SrplBusinessOperationKindIr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SrplBusinessOperationKindIr {
    Read {
        source: QualifiedName,
        binding: String,
        cardinality: Cardinality,
        predicates: Vec<SrplPredicateIr>,
    },
    Assert {
        predicate: SrplPredicateIr,
        failure_code: String,
    },
    Update {
        target: QualifiedName,
        predicates: Vec<SrplPredicateIr>,
        assignments: Vec<SrplAssignmentIr>,
        affected_rows_exact: Option<u64>,
    },
    Emit {
        stream: String,
        values: Vec<SrplEmitValueIr>,
    },
    Raise {
        code: String,
    },
}

impl SrplBusinessOperationKindIr {
    fn validate(&self) -> AndromedaResult<()> {
        match self {
            Self::Read {
                source,
                binding,
                predicates,
                ..
            } => {
                validate_qualified_name(source, "SRPL read source")?;
                validate_symbol(binding, "SRPL read binding")?;
                for predicate in predicates {
                    predicate.validate()?;
                }
            }
            Self::Assert {
                predicate,
                failure_code,
            } => {
                predicate.validate()?;
                validate_symbol(failure_code, "SRPL assertion failure code")?;
            }
            Self::Update {
                target,
                predicates,
                assignments,
                affected_rows_exact,
            } => {
                validate_qualified_name(target, "SRPL update target")?;
                if assignments.is_empty() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Srpl,
                        "SRPL update operation must declare at least one assignment",
                    ));
                }
                for predicate in predicates {
                    predicate.validate()?;
                }
                for assignment in assignments {
                    assignment.validate()?;
                }
                if matches!(affected_rows_exact, Some(0)) {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Srpl,
                        "SRPL update affected rows must be greater than zero",
                    ));
                }
            }
            Self::Emit { stream, values } => {
                validate_symbol(stream, "SRPL emit stream")?;
                if values.is_empty() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Srpl,
                        "SRPL emit operation must declare at least one value",
                    ));
                }
                for value in values {
                    value.validate()?;
                }
            }
            Self::Raise { code } => {
                validate_symbol(code, "SRPL raise code")?;
            }
        }

        Ok(())
    }
}

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
    fn validate(&self) -> AndromedaResult<()> {
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
    fn validate(&self) -> AndromedaResult<()> {
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
    fn validate(&self) -> AndromedaResult<()> {
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
    /// Boolean constant — `true` or `false`.
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
    pub fn is_compatible_with(&self, scalar: &andromeda_core::ScalarType) -> bool {
        use andromeda_core::ScalarType;
        match (self, scalar) {
            (Self::Bool(_), ScalarType::Bool) => true,
            (Self::Int64(_), ScalarType::I8)
            | (Self::Int64(_), ScalarType::I16)
            | (Self::Int64(_), ScalarType::I32)
            | (Self::Int64(_), ScalarType::I64)
            | (Self::Int64(_), ScalarType::I128) => true,
            (Self::Uint64(_), ScalarType::U8)
            | (Self::Uint64(_), ScalarType::U16)
            | (Self::Uint64(_), ScalarType::U32)
            | (Self::Uint64(_), ScalarType::U64)
            | (Self::Uint64(_), ScalarType::U128) => true,
            (Self::Decimal { .. }, ScalarType::Decimal(_)) => true,
            _ => false,
        }
    }

    /// Validate that the decimal invariant `scale <= 18` holds.
    pub fn validate(&self) -> AndromedaResult<()> {
        if let Self::Decimal { scale, .. } = self {
            if *scale > 18 {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Srpl,
                    "SRPL constant decimal scale must not exceed 18",
                ));
            }
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
    // ------------------------------------------------------------------ //
    // Existing variants — preserved for backward compatibility.            //
    // The `Bool(b)` variant is deprecated in favour of                    //
    // `Constant(ConstantLiteral::Bool(b))` but must not be removed until  //
    // all call-sites have been migrated (Wave 14 task W14-1).             //
    // ------------------------------------------------------------------ //
    /// Reference to a named procedure input parameter.
    Input(String),

    /// Reference to a field of a bound row-set.
    Field { binding: String, field: String },

    /// **Deprecated** — use `Constant(ConstantLiteral::Bool(_))` instead.
    /// Kept for lowering pipeline backward compatibility.
    Bool(bool),

    /// `binding.field - :input` convenience shorthand (inventory pattern).
    SubtractInput {
        binding: String,
        field: String,
        input: String,
    },

    // ------------------------------------------------------------------ //
    // New variants added by v1-srpl-constant-folding-s03-design           //
    // ------------------------------------------------------------------ //
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

    fn validate(&self) -> AndromedaResult<()> {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplProcedureContractMetadata {
    pub object_id: CatalogObjectId,
    pub procedure_id: ProcedureId,
    pub catalog_version: CatalogVersion,
    pub stats_version: StatsVersion,
    pub protocol_layout: ProtocolLayoutRef,
    pub structured_inputs: Vec<QualifiedName>,
    pub required_permissions: Vec<String>,
    pub transaction_policy: TransactionPolicy,
    pub compatibility_policy: CompatibilityPolicy,
    pub result_metadata_policy: ResultMetadataPolicy,
    pub error_policy: ProcedureErrorPolicy,
    pub multi_result_policy: MultiResultPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutableProcedurePlan {
    pub procedure_name: QualifiedName,
    pub body: BoundSrplBodyPlan,
    pub evidence: SrplCatalogBindingEvidence,
}

impl ExecutableProcedurePlan {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.evidence.validate()?;
        self.body.validate()?;
        if self.procedure_name != self.evidence.procedure_object.name {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "executable SRPL plan procedure name must match binding evidence",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundSrplBodyPlan {
    pub operations: Vec<BoundSrplOperationPlan>,
}

impl BoundSrplBodyPlan {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.operations.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "executable SRPL body plan must contain deterministic operations",
            ));
        }

        if self.operations.len() > MAX_SRPL_BODY_OPERATIONS {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "executable SRPL body plan exceeds the bounded operation limit",
            ));
        }

        for (expected_ordinal, operation) in self.operations.iter().enumerate() {
            if operation.ordinal() != expected_ordinal as u32 {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Srpl,
                    "executable SRPL body plan ordinals must be dense and zero-based",
                ));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundSrplOperationPlan {
    ReadTable {
        ordinal: u32,
        source: CatalogObjectRef,
        binding: String,
        cardinality: Cardinality,
        predicates: Vec<SrplPredicateIr>,
    },
    Assert {
        ordinal: u32,
        predicate: SrplPredicateIr,
        failure_code: String,
    },
    UpdateTable {
        ordinal: u32,
        target: CatalogObjectRef,
        predicates: Vec<SrplPredicateIr>,
        assignments: Vec<SrplAssignmentIr>,
        affected_rows_exact: Option<u64>,
    },
    Emit {
        ordinal: u32,
        stream: String,
        values: Vec<SrplEmitValueIr>,
    },
    Raise {
        ordinal: u32,
        code: String,
    },
}

impl BoundSrplOperationPlan {
    pub fn ordinal(&self) -> u32 {
        match self {
            Self::ReadTable { ordinal, .. }
            | Self::Assert { ordinal, .. }
            | Self::UpdateTable { ordinal, .. }
            | Self::Emit { ordinal, .. }
            | Self::Raise { ordinal, .. } => *ordinal,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplCatalogBindingEvidence {
    pub catalog_version: CatalogVersion,
    pub procedure_object: CatalogObjectRef,
    pub procedure_contract: ProcedureContractRef,
    /// Catalog objects referenced by the procedure body (tables read or
    /// updated, structured objects emitted as result streams). The list is
    /// ordered by the binder so the evidence is deterministic across
    /// equivalent SRPL inputs.
    pub bound_objects: Vec<SrplObjectBindingEvidence>,
}

impl SrplCatalogBindingEvidence {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.procedure_object
            .validate_for_definition(andromeda_catalog::ObjectKind::Procedure)?;
        self.procedure_contract.validate()?;

        if self.procedure_object.catalog_version != self.catalog_version
            || self.procedure_contract.catalog_version != self.catalog_version
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "SRPL executable plan evidence requires exact catalog version match",
            ));
        }

        if self.procedure_contract.contract_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "SRPL executable plan binding hashes must not be zero",
            ));
        }

        for bound in &self.bound_objects {
            bound.validate(bound.kind)?;
            if bound.object.catalog_version != self.catalog_version {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "SRPL executable plan evidence requires exact catalog version match",
                ));
            }
        }

        Ok(())
    }

    /// Returns evidence for the first bound object whose qualified name matches.
    pub fn find_bound_object(&self, name: &QualifiedName) -> Option<&SrplObjectBindingEvidence> {
        self.bound_objects
            .iter()
            .find(|bound| &bound.object.name == name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplObjectBindingEvidence {
    pub object: CatalogObjectRef,
    pub shape_hash: ContractHash,
    pub kind: ObjectKind,
}

impl SrplObjectBindingEvidence {
    pub fn validate(&self, expected_kind: ObjectKind) -> AndromedaResult<()> {
        self.object.validate_for_definition(expected_kind)?;
        if self.kind != expected_kind {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "SRPL object binding kind must match the bound catalog object",
            ));
        }
        if self.shape_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "SRPL object binding shape hash must not be zero",
            ));
        }
        Ok(())
    }
}

fn validate_qualified_name(name: &QualifiedName, context: &str) -> AndromedaResult<()> {
    if name.parts().is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            format!("{context} must not be empty"),
        ));
    }

    Ok(())
}

fn validate_symbol(value: &str, context: &str) -> AndromedaResult<()> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            format!("{context} must not be empty"),
        ));
    };

    if !(first.is_ascii_alphabetic() || first == '_')
        || chars.any(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
    {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            format!("{context} must be an ASCII identifier"),
        ));
    }

    Ok(())
}
