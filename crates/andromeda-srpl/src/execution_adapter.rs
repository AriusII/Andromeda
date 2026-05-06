//! SRPL execution adapter contracts.
//!
//! This module defines the narrow boundary between catalog-bound SRPL plans and
//! any future execution engine.  It intentionally carries no storage, transport,
//! or physical runtime dependency: adapters receive typed, bounded requests over
//! catalog object references and procedure contract references only.

use std::num::NonZeroU64;

use andromeda_catalog::{CatalogObjectRef, ObjectKind, ProcedureContractRef};
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::procedure_model::{
    Cardinality, MAX_SRPL_BODY_OPERATIONS, SrplAssignmentIr, SrplEmitValueIr, SrplPredicateIr,
};

/// Trait defining the binding environment contract for SRPL predicate evaluation.
///
/// The binding environment provides deterministic, bounded access to:
/// - Input parameters (from procedure inputs)
/// - Read bindings (from prior READ operations)
/// - Local variables (future extension)
///
/// Adapters receive a read-only reference to this environment to evaluate
/// predicates without side effects or non-determinism.
pub trait SrplBindingEnvironment: Send + Sync {
    /// Retrieve an input parameter value by name.
    /// Returns None if the input is not bound.
    fn get_input(&self, name: &str) -> Option<SrplBoundValue>;

    /// Retrieve a field value from a bound row.
    /// Returns None if the binding or field does not exist.
    fn get_field_from_binding(
        &self,
        binding: &str,
        row_index: usize,
        field: &str,
    ) -> Option<SrplBoundValue>;

    /// Retrieve the number of rows in a binding.
    /// Returns None if the binding does not exist.
    fn binding_row_count(&self, binding: &str) -> Option<usize>;
}

/// Type alias for field values in the binding environment.
/// Implements equality and comparison for predicate evaluation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SrplBoundValue {
    Integer(i64),
    String(String),
    Bool(bool),
    Null,
}

/// Empty trait for future extensibility (row types handled via binding environment)
pub trait SrplBoundRow: Send + Sync {}

/// Bounded row-count contract for SRPL adapter operations.
///
/// There is deliberately no unbounded variant.  Construction accepts plain
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

    fn validate_for_cardinality(self, cardinality: Cardinality) -> AndromedaResult<()> {
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

    fn permits_actual(self, actual_rows: u64) -> bool {
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
/// runtime value environment.  The request still carries the lowered predicate
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

/// Adapter-visible typed failure surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SrplExecutionFailure {
    SemanticViolation(String),
    CatalogBindingViolation(String),
    ContractViolation(String),
    CardinalityViolation {
        expected: Cardinality,
        bound: SrplRowBound,
        actual_rows: u64,
    },
    ResourceLimitExceeded(String),
    AdapterRejected(String),
}

impl SrplExecutionFailure {
    pub fn into_andromeda_error(self) -> AndromedaError {
        match self {
            Self::SemanticViolation(message) => {
                AndromedaError::new(AndromedaErrorKind::Srpl, message)
            }
            Self::CatalogBindingViolation(message) => {
                AndromedaError::new(AndromedaErrorKind::Catalog, message)
            }
            Self::ContractViolation(message) => {
                AndromedaError::new(AndromedaErrorKind::Contract, message)
            }
            Self::CardinalityViolation { .. } => AndromedaError::new(
                AndromedaErrorKind::Execution,
                "SRPL adapter operation violated its cardinality or row bound",
            ),
            Self::ResourceLimitExceeded(message) => {
                AndromedaError::new(AndromedaErrorKind::Resource, message)
            }
            Self::AdapterRejected(message) => {
                AndromedaError::new(AndromedaErrorKind::Execution, message)
            }
        }
    }
}

impl From<AndromedaError> for SrplExecutionFailure {
    fn from(error: AndromedaError) -> Self {
        let message = error.message().to_string();
        match error.kind() {
            AndromedaErrorKind::Catalog => Self::CatalogBindingViolation(message),
            AndromedaErrorKind::Contract => Self::ContractViolation(message),
            AndromedaErrorKind::Resource => Self::ResourceLimitExceeded(message),
            AndromedaErrorKind::Srpl => Self::SemanticViolation(message),
            _ => Self::AdapterRejected(message),
        }
    }
}

/// Result of a typed assertion adapter call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SrplAssertResult {
    pub passed: bool,
}

impl SrplAssertResult {
    pub const fn new(passed: bool) -> Self {
        Self { passed }
    }
}

/// Result of a typed read/scan adapter call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplReadResult<Row> {
    pub rows: Vec<Row>,
}

impl<Row> SrplReadResult<Row> {
    pub fn new(
        rows: Vec<Row>,
        cardinality: Cardinality,
        bound: SrplRowBound,
    ) -> Result<Self, SrplExecutionFailure> {
        validate_actual_rows(rows.len() as u64, cardinality, bound)?;
        Ok(Self { rows })
    }
}

/// Result of a typed update adapter call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SrplUpdateResult {
    pub affected_rows: u64,
}

impl SrplUpdateResult {
    pub fn new(affected_rows: u64, bound: SrplRowBound) -> Result<Self, SrplExecutionFailure> {
        if !bound.permits_actual(affected_rows) {
            return Err(SrplExecutionFailure::CardinalityViolation {
                expected: Cardinality::Many,
                bound,
                actual_rows: affected_rows,
            });
        }
        Ok(Self { affected_rows })
    }
}

/// Result of a typed emit adapter call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SrplEmitResult {
    pub emitted_rows: u64,
}

impl SrplEmitResult {
    pub fn new(
        emitted_rows: u64,
        cardinality: Cardinality,
        bound: SrplRowBound,
    ) -> Result<Self, SrplExecutionFailure> {
        validate_actual_rows(emitted_rows, cardinality, bound)?;
        Ok(Self { emitted_rows })
    }
}

/// Adapter trait for bounded typed reads/scans.
///
/// The read adapter receives a binding context that includes:
/// - Input parameter values (from procedure inputs)
/// - Previously bound read results (from prior Read operations)
/// - Local variable bindings
///
/// Predicates in the request are evaluated against this environment to filter rows.
pub trait SrplTypedReadAdapter {
    type Row;

    fn read_typed(
        &mut self,
        request: SrplReadRequest,
        environment: &dyn SrplBindingEnvironment,
    ) -> Result<SrplReadResult<Self::Row>, SrplExecutionFailure>;
}

/// Adapter trait for lowered SRPL assertion predicates.
///
/// The assertion adapter receives a binding context to evaluate predicates
/// against the current runtime state (inputs and bindings).
pub trait SrplAssertionAdapter {
    fn assert_typed(
        &mut self,
        request: SrplAssertRequest,
        environment: &dyn SrplBindingEnvironment,
    ) -> Result<SrplAssertResult, SrplExecutionFailure>;
}

/// Adapter trait for bounded typed updates.
///
/// The update adapter receives a binding context to:
/// - Validate that predicates can bind to the environment
/// - In production: filter rows before applying assignments
/// - Maintain deterministic, all-or-nothing semantics
pub trait SrplTypedUpdateAdapter {
    fn update_typed(
        &mut self,
        request: SrplUpdateRequest,
        environment: &dyn SrplBindingEnvironment,
    ) -> Result<SrplUpdateResult, SrplExecutionFailure>;
}

/// Adapter trait for bounded typed result emission.
///
/// The emit adapter receives a binding context for consistency,
/// though it primarily uses the context for diagnostic/tracing purposes.
pub trait SrplTypedEmitAdapter {
    fn emit_typed(
        &mut self,
        request: SrplEmitRequest,
        environment: &dyn SrplBindingEnvironment,
    ) -> Result<SrplEmitResult, SrplExecutionFailure>;
}

/// Adapter trait for reporting typed SRPL semantic/execution failures.
///
/// The failure adapter receives a binding context for recovery purposes.
pub trait SrplFailureAdapter {
    fn fail_typed(
        &mut self,
        request: SrplFailureRequest,
        environment: &dyn SrplBindingEnvironment,
    ) -> Result<(), SrplExecutionFailure>;
}

fn validate_actual_rows(
    actual_rows: u64,
    cardinality: Cardinality,
    bound: SrplRowBound,
) -> Result<(), SrplExecutionFailure> {
    if cardinality.permits_exact_row_count(actual_rows) && bound.permits_actual(actual_rows) {
        return Ok(());
    }

    Err(SrplExecutionFailure::CardinalityViolation {
        expected: cardinality,
        bound,
        actual_rows,
    })
}

fn invalid_bound(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Srpl, message)
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

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_catalog::{ObjectKind, QualifiedName};
    use andromeda_core::{
        AndromedaErrorKind, CatalogObjectId, CatalogVersion, ContractHash, ProcedureId, ScalarType,
        TypeDescriptor,
    };

    fn procedure() -> ProcedureContractRef {
        ProcedureContractRef {
            procedure_id: ProcedureId::new(11),
            contract_hash: ContractHash::test_vector(0x11),
            catalog_version: CatalogVersion::new(7),
        }
    }

    fn context(ordinal: u32) -> SrplOperationContext {
        SrplOperationContext::new(procedure(), ordinal).unwrap()
    }

    fn catalog_object(kind: ObjectKind) -> CatalogObjectRef {
        CatalogObjectRef {
            object_id: CatalogObjectId::new(22),
            name: QualifiedName::parse("Inventory.Stock").unwrap(),
            kind,
            catalog_version: CatalogVersion::new(7),
        }
    }

    #[test]
    fn execution_adapter_bounded_read_request_constructs() {
        let bound = SrplRowBound::at_most(8).unwrap();
        let request = SrplReadRequest::new(
            context(0),
            catalog_object(ObjectKind::Table),
            Cardinality::Many,
            bound,
            vec![SrplPredicateIr::InputEqualsField {
                input: "ProductId".to_string(),
                binding: "stock".to_string(),
                field: "ProductId".to_string(),
            }],
        )
        .unwrap();

        assert_eq!(request.row_bound.get(), 8);
        assert_eq!(request.source.kind, ObjectKind::Table);
    }

    #[test]
    fn execution_adapter_bounded_update_request_constructs() {
        let request = SrplUpdateRequest::new(
            context(1),
            catalog_object(ObjectKind::Table),
            SrplRowBound::exact(1).unwrap(),
            Vec::new(),
            vec![SrplAssignmentIr {
                field: "Reserved".to_string(),
                value: crate::procedure_model::SrplValueIr::Bool(true),
            }],
        )
        .unwrap();

        assert!(request.affected_rows.is_exact());
        assert_eq!(request.assignments.len(), 1);
    }

    #[test]
    fn execution_adapter_assert_request_constructs() {
        let request = SrplAssertRequest::new(
            context(1),
            SrplPredicateIr::FieldGreaterThanOrEqualInput {
                binding: "stock".to_string(),
                field: "Available".to_string(),
                input: "Quantity".to_string(),
            },
            "InsufficientStock",
        )
        .unwrap();

        assert_eq!(request.failure_code, "InsufficientStock");
        assert_eq!(request.context.ordinal, 1);
    }

    #[test]
    fn execution_adapter_bounded_emit_request_constructs() {
        let request = SrplEmitRequest::new(
            context(2),
            "Reservation",
            Cardinality::One,
            SrplRowBound::exact(1).unwrap(),
            vec![SrplEmitValueIr {
                column: "Reserved".to_string(),
                value: crate::procedure_model::SrplValueIr::Bool(true),
            }],
        )
        .unwrap();

        assert_eq!(request.stream, "Reservation");
        assert_eq!(request.row_bound.get(), 1);
    }

    #[test]
    fn execution_adapter_rejects_unbounded_and_zero_bounds() {
        assert_eq!(
            SrplRowBound::exact(0).unwrap_err().kind(),
            AndromedaErrorKind::Srpl
        );
        assert_eq!(
            SrplRowBound::at_most(0).unwrap_err().kind(),
            AndromedaErrorKind::Srpl
        );
        assert_eq!(
            SrplRowBound::required_at_most(None).unwrap_err().kind(),
            AndromedaErrorKind::Srpl
        );
    }

    #[test]
    fn execution_adapter_rejects_non_table_read_target() {
        let error = SrplReadRequest::new(
            context(0),
            catalog_object(ObjectKind::Procedure),
            Cardinality::Many,
            SrplRowBound::at_most(1).unwrap(),
            Vec::new(),
        )
        .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    }

    #[test]
    fn execution_adapter_rejects_zero_procedure_binding() {
        let procedure = ProcedureContractRef {
            procedure_id: ProcedureId::new(0),
            contract_hash: ContractHash::test_vector(0x11),
            catalog_version: CatalogVersion::new(7),
        };

        assert_eq!(
            SrplOperationContext::new(procedure, 0).unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );
    }

    #[test]
    fn execution_adapter_results_map_bound_failures_without_panic() {
        let bound = SrplRowBound::exact(1).unwrap();

        let failure = SrplReadResult::<()>::new(vec![(), ()], Cardinality::One, bound).unwrap_err();

        assert!(matches!(
            failure.clone(),
            SrplExecutionFailure::CardinalityViolation {
                expected: Cardinality::One,
                actual_rows: 2,
                ..
            }
        ));
        assert_eq!(
            failure.into_andromeda_error().kind(),
            AndromedaErrorKind::Execution
        );
    }

    #[test]
    fn execution_adapter_keeps_core_types_only_for_shape_tests() {
        let descriptor = TypeDescriptor::required(ScalarType::Bool);

        assert!(descriptor.validate().is_ok());
    }
}
