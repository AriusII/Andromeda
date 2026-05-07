use std::num::NonZeroU64;

use andromeda_catalog::{CatalogObjectRef, ObjectKind, ProcedureContractRef};
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{
    identifier::validate_srpl_identifier as validate_symbol,
    procedure_model::{
        Cardinality, MAX_SRPL_BODY_OPERATIONS, SrplAssignmentIr, SrplEmitValueIr, SrplPredicateIr,
    },
};

use super::diagnostics::SrplExecutionFailure;

/// Bounded row-count contract for SRPL adapter operations.
///
/// There is deliberately no unbounded variant. Construction accepts plain
/// integers for compiler/lowering convenience and rejects zero at the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SrplRowBound {
    /// The operation must observe exactly this many rows.
    Exact(NonZeroU64),
    /// The operation may observe no more than this many rows.
    AtMost(NonZeroU64),
}

impl SrplRowBound {
    pub fn exact(row_count: u64) -> AndromedaResult<Self> {
        NonZeroU64::new(row_count)
            .map(Self::Exact)
            .ok_or_else(|| invalid_bound("SRPL exact row bound must be greater than zero"))
    }

    pub fn at_most(max_rows: u64) -> AndromedaResult<Self> {
        NonZeroU64::new(max_rows)
            .map(Self::AtMost)
            .ok_or_else(|| invalid_bound("SRPL upper row bound must be greater than zero"))
    }

    /// Converts an external optional upper bound into the bounded adapter shape.
    /// `None` is rejected so scans cannot become unbounded by omission.
    pub fn required_at_most(max_rows: Option<u64>) -> AndromedaResult<Self> {
        match max_rows {
            Some(max_rows) => Self::at_most(max_rows),
            None => Err(invalid_bound(
                "SRPL adapter requests must declare a bounded row count",
            )),
        }
    }

    pub const fn get(self) -> u64 {
        match self {
            Self::Exact(value) | Self::AtMost(value) => value.get(),
        }
    }

    pub const fn is_exact(self) -> bool {
        matches!(self, Self::Exact(_))
    }

    pub(super) fn validate_for_cardinality(self, cardinality: Cardinality) -> AndromedaResult<()> {
        match self {
            Self::Exact(row_count) => {
                if !cardinality.permits_exact_row_count(row_count.get()) {
                    return Err(invalid_bound(
                        "SRPL exact row bound conflicts with operation cardinality",
                    ));
                }
            }
            Self::AtMost(max_rows) => {
                if !cardinality.permits_row_count_max(max_rows.get()) {
                    return Err(invalid_bound(
                        "SRPL upper row bound conflicts with operation cardinality",
                    ));
                }
            }
        }

        Ok(())
    }

    pub(super) fn permits_actual(self, actual_rows: u64) -> bool {
        match self {
            Self::Exact(row_count) => actual_rows == row_count.get(),
            Self::AtMost(max_rows) => actual_rows <= max_rows.get(),
        }
    }
}

/// Procedure-scoped adapter context for a single lowered SRPL operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SrplOperationContext {
    pub procedure: ProcedureContractRef,
    pub ordinal: u32,
}

impl SrplOperationContext {
    pub fn new(procedure: ProcedureContractRef, ordinal: u32) -> AndromedaResult<Self> {
        let context = Self { procedure, ordinal };
        context.validate()?;
        Ok(context)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.procedure.validate()?;
        if self.ordinal as usize >= MAX_SRPL_BODY_OPERATIONS {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "SRPL adapter operation ordinal exceeds the bounded operation limit",
            ));
        }

        Ok(())
    }
}

/// Typed, bounded table read/scan request over a catalog-bound table object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplReadRequest {
    pub context: SrplOperationContext,
    pub source: CatalogObjectRef,
    pub cardinality: Cardinality,
    pub row_bound: SrplRowBound,
    pub predicates: Vec<SrplPredicateIr>,
}

impl SrplReadRequest {
    pub fn new(
        context: SrplOperationContext,
        source: CatalogObjectRef,
        cardinality: Cardinality,
        row_bound: SrplRowBound,
        predicates: Vec<SrplPredicateIr>,
    ) -> AndromedaResult<Self> {
        let request = Self {
            context,
            source,
            cardinality,
            row_bound,
            predicates,
        };
        request.validate()?;
        Ok(request)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.context.validate()?;
        self.source.validate_for_definition(ObjectKind::Table)?;
        self.row_bound.validate_for_cardinality(self.cardinality)?;
        Ok(())
    }
}

/// Typed, bounded table update request over a catalog-bound table object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplUpdateRequest {
    pub context: SrplOperationContext,
    pub target: CatalogObjectRef,
    pub affected_rows: SrplRowBound,
    pub predicates: Vec<SrplPredicateIr>,
    pub assignments: Vec<SrplAssignmentIr>,
}

impl SrplUpdateRequest {
    pub fn new(
        context: SrplOperationContext,
        target: CatalogObjectRef,
        affected_rows: SrplRowBound,
        predicates: Vec<SrplPredicateIr>,
        assignments: Vec<SrplAssignmentIr>,
    ) -> AndromedaResult<Self> {
        let request = Self {
            context,
            target,
            affected_rows,
            predicates,
            assignments,
        };
        request.validate()?;
        Ok(request)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.context.validate()?;
        self.target.validate_for_definition(ObjectKind::Table)?;
        if self.assignments.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "SRPL adapter update request must declare at least one assignment",
            ));
        }
        Ok(())
    }
}

/// Typed, bounded result emission request scoped to a procedure contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplEmitRequest {
    pub context: SrplOperationContext,
    pub stream: String,
    pub cardinality: Cardinality,
    pub row_bound: SrplRowBound,
    pub values: Vec<SrplEmitValueIr>,
}

impl SrplEmitRequest {
    pub fn new(
        context: SrplOperationContext,
        stream: impl Into<String>,
        cardinality: Cardinality,
        row_bound: SrplRowBound,
        values: Vec<SrplEmitValueIr>,
    ) -> AndromedaResult<Self> {
        let request = Self {
            context,
            stream: stream.into(),
            cardinality,
            row_bound,
            values,
        };
        request.validate()?;
        Ok(request)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.context.validate()?;
        validate_symbol(&self.stream, "SRPL adapter emit stream")?;
        self.row_bound.validate_for_cardinality(self.cardinality)?;
        if self.values.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "SRPL adapter emit request must declare at least one value",
            ));
        }
        Ok(())
    }
}

/// Typed assertion request scoped to a procedure operation.
///
/// The interpreter deliberately delegates predicate truth evaluation to the
/// adapter boundary because this crate has no storage row representation and no
/// runtime value environment. The request still carries the lowered predicate
/// and failure code deterministically, so adapters cannot reinterpret SRPL as
/// ad hoc query text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplAssertRequest {
    pub context: SrplOperationContext,
    pub predicate: SrplPredicateIr,
    pub failure_code: String,
}

impl SrplAssertRequest {
    pub fn new(
        context: SrplOperationContext,
        predicate: SrplPredicateIr,
        failure_code: impl Into<String>,
    ) -> AndromedaResult<Self> {
        let request = Self {
            context,
            predicate,
            failure_code: failure_code.into(),
        };
        request.validate()?;
        Ok(request)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.context.validate()?;
        validate_symbol(&self.failure_code, "SRPL adapter assertion failure code")
    }
}

/// Typed failure request for assertion/raise surfaces scoped to a procedure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplFailureRequest {
    pub context: SrplOperationContext,
    pub code: String,
    pub failure: SrplExecutionFailure,
}

impl SrplFailureRequest {
    pub fn new(
        context: SrplOperationContext,
        code: impl Into<String>,
        failure: SrplExecutionFailure,
    ) -> AndromedaResult<Self> {
        let request = Self {
            context,
            code: code.into(),
            failure,
        };
        request.validate()?;
        Ok(request)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.context.validate()?;
        validate_symbol(&self.code, "SRPL adapter failure code")
    }
}

fn invalid_bound(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Srpl, message)
}
