//! SRPL execution adapters for typed IR operations.
//!
//! This module implements the 5 core execution adapter traits that bridge
//! the stateless SRPL IR interpreter to runtime catalog, transaction, and
//! stream services:
//!
//! 1. **SrplTypedReadAdapter** — Catalog table scans with predicate binding
//! 2. **SrplTypedUpdateAdapter** — In-transaction updates (all-or-nothing)
//! 3. **SrplTypedEmitAdapter** — Result stream emission (respects backpressure)
//! 4. **SrplAssertionAdapter** — Predicate evaluation over typed environment
//! 5. **SrplFailureAdapter** — Error mapping and recovery semantics
//!
//! All adapters maintain:
//! - Type cardinality through operations (1:1 or N:M preservation)
//! - Error determinism and reproducibility
//! - Predicate binding via scoped type environment (no SQL ad-hoc)
//! - Stream backpressure (no unbounded buffering)
//! - Transaction atomicity (all-or-nothing per invocation)

use std::collections::BTreeMap;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_srpl::{
    execution_adapter::{
        SrplAssertRequest, SrplAssertResult, SrplAssertionAdapter, SrplBindingEnvironment,
        SrplBoundValue, SrplEmitRequest, SrplEmitResult, SrplExecutionFailure, SrplFailureAdapter,
        SrplFailureRequest, SrplReadRequest, SrplReadResult, SrplTypedEmitAdapter,
        SrplTypedReadAdapter, SrplTypedUpdateAdapter, SrplUpdateRequest, SrplUpdateResult,
    },
    procedure_model::SrplPredicateIr,
};

/// Mock structured object for reads (replaces catalog row type).
/// In production, this would be bound to actual catalog storage types.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StructuredObject {
    pub fields: BTreeMap<String, FieldValue>,
}

impl StructuredObject {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_field(mut self, name: impl Into<String>, value: FieldValue) -> Self {
        self.fields.insert(name.into(), value);
        self
    }

    pub fn get_field(&self, name: &str) -> Option<&FieldValue> {
        self.fields.get(name)
    }
}

/// Typed field value supporting predicate evaluation and emit operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    Integer(i64),
    String(String),
    Bool(bool),
    Null,
}

impl FieldValue {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Convert to SrplBoundValue for trait compatibility
    pub fn to_srpl_bound_value(&self) -> SrplBoundValue {
        match self {
            Self::Integer(v) => SrplBoundValue::Integer(*v),
            Self::String(s) => SrplBoundValue::String(s.clone()),
            Self::Bool(b) => SrplBoundValue::Bool(*b),
            Self::Null => SrplBoundValue::Null,
        }
    }

    /// Convert from SrplBoundValue
    pub fn from_srpl_bound_value(value: SrplBoundValue) -> Self {
        match value {
            SrplBoundValue::Integer(v) => Self::Integer(v),
            SrplBoundValue::String(s) => Self::String(s),
            SrplBoundValue::Bool(b) => Self::Bool(b),
            SrplBoundValue::Null => Self::Null,
        }
    }
}

impl From<i64> for FieldValue {
    fn from(v: i64) -> Self {
        Self::Integer(v)
    }
}

impl From<String> for FieldValue {
    fn from(v: String) -> Self {
        Self::String(v)
    }
}

impl From<bool> for FieldValue {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}

/// Scoped type environment for predicate binding.
/// Maintains bindings (read results) and input parameters throughout
/// a procedure execution, ensuring predicates evaluate deterministically.
#[derive(Debug, Clone, Default)]
pub struct SrplTypedEnvironment {
    /// Bound results from read operations: binding_name -> objects
    bindings: BTreeMap<String, Vec<StructuredObject>>,
    /// Input parameters: name -> value
    inputs: BTreeMap<String, FieldValue>,
}

impl SrplTypedEnvironment {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an input parameter to the environment.
    pub fn add_input(&mut self, name: impl Into<String>, value: FieldValue) {
        self.inputs.insert(name.into(), value);
    }

    /// Bind a read result (multiple rows) to a name.
    pub fn bind_read(&mut self, binding_name: impl Into<String>, rows: Vec<StructuredObject>) {
        self.bindings.insert(binding_name.into(), rows);
    }

    /// Retrieve an input parameter.
    pub fn get_input_internal(&self, name: &str) -> Option<&FieldValue> {
        self.inputs.get(name)
    }

    /// Retrieve bound rows.
    pub fn get_binding_internal(&self, name: &str) -> Option<&Vec<StructuredObject>> {
        self.bindings.get(name)
    }
}

impl SrplBindingEnvironment for SrplTypedEnvironment {
    fn get_input(&self, name: &str) -> Option<SrplBoundValue> {
        self.inputs.get(name).map(|fv| fv.to_srpl_bound_value())
    }

    fn get_field_from_binding(
        &self,
        binding: &str,
        row_index: usize,
        field: &str,
    ) -> Option<SrplBoundValue> {
        self.bindings
            .get(binding)
            .and_then(|rows| rows.get(row_index))
            .and_then(|row| row.get_field(field))
            .map(|fv| fv.to_srpl_bound_value())
    }

    fn binding_row_count(&self, binding: &str) -> Option<usize> {
        self.bindings.get(binding).map(|rows| rows.len())
    }
}

/// Transaction-scoped state for SRPL execution.
/// Maintains read-write visibility and all-or-nothing semantics.
#[derive(Debug, Clone, Default)]
pub struct SrplTransactionContext {
    /// Committed updates pending transaction commit.
    pending_updates: BTreeMap<String, Vec<(usize, StructuredObject)>>,
    /// Flag indicating if transaction is aborted.
    is_aborted: bool,
}

impl SrplTransactionContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an update to a table. Returns error if transaction is aborted.
    pub fn record_update(
        &mut self,
        table_id: &str,
        row_index: usize,
        updated_row: StructuredObject,
    ) -> AndromedaResult<()> {
        if self.is_aborted {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "SRPL transaction aborted; cannot record further updates",
            ));
        }

        self.pending_updates
            .entry(table_id.to_string())
            .or_insert_with(Vec::new)
            .push((row_index, updated_row));

        Ok(())
    }

    /// Abort the transaction (idempotent).
    pub fn abort(&mut self) {
        self.is_aborted = true;
    }

    /// Check if transaction is aborted.
    pub fn is_aborted(&self) -> bool {
        self.is_aborted
    }

    /// Retrieve all pending updates (for commit verification).
    pub fn pending_updates(&self) -> &BTreeMap<String, Vec<(usize, StructuredObject)>> {
        &self.pending_updates
    }
}

/// Stream backpressure controller.
/// Respects maximum buffered rows and enforces bounded emission.
#[derive(Debug, Clone)]
pub struct SrplStreamBackpressure {
    /// Maximum rows buffered before backpressure triggers.
    max_buffered_rows: usize,
    /// Currently buffered rows across all streams.
    buffered_rows: usize,
}

impl SrplStreamBackpressure {
    pub fn new(max_buffered_rows: usize) -> Self {
        Self {
            max_buffered_rows,
            buffered_rows: 0,
        }
    }

    /// Attempt to buffer rows. Returns error if backpressure threshold exceeded.
    pub fn buffer_rows(&mut self, count: usize) -> AndromedaResult<()> {
        if self.buffered_rows + count > self.max_buffered_rows {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                format!(
                    "SRPL stream backpressure exceeded: {} rows buffered, max {}",
                    self.buffered_rows + count,
                    self.max_buffered_rows
                ),
            ));
        }
        self.buffered_rows += count;
        Ok(())
    }

    /// Release buffered rows after emission.
    pub fn release_rows(&mut self, count: usize) {
        self.buffered_rows = self.buffered_rows.saturating_sub(count);
    }
}

/// Complete SRPL execution adapter combining all 5 traits.
/// Maintains transaction scope, typed environment, and stream backpressure.
pub struct SrplExecutionAdapter {
    /// Scoped type environment for predicates and bindings.
    environment: SrplTypedEnvironment,
    /// Transaction-scoped update state.
    transaction: SrplTransactionContext,
    /// Stream backpressure controller.
    backpressure: SrplStreamBackpressure,
    /// Accumulated failure records (for recovery).
    failures: Vec<(String, SrplExecutionFailure)>,
}

impl SrplExecutionAdapter {
    pub fn new(max_buffered_rows: usize) -> Self {
        Self {
            environment: SrplTypedEnvironment::new(),
            transaction: SrplTransactionContext::new(),
            backpressure: SrplStreamBackpressure::new(max_buffered_rows),
            failures: Vec::new(),
        }
    }

    /// Add an input to the environment.
    pub fn add_input(&mut self, name: impl Into<String>, value: FieldValue) {
        self.environment.add_input(name, value);
    }

    /// Get mutable reference to environment for testing.
    pub fn environment_mut(&mut self) -> &mut SrplTypedEnvironment {
        &mut self.environment
    }

    /// Get reference to transaction context.
    pub fn transaction(&self) -> &SrplTransactionContext {
        &self.transaction
    }

    /// Abort the current transaction.
    pub fn abort_transaction(&mut self) {
        self.transaction.abort();
    }

    /// Check if transaction is aborted.
    pub fn is_transaction_aborted(&self) -> bool {
        self.transaction.is_aborted()
    }

    /// Retrieve failure records.
    pub fn failures(&self) -> &[(String, SrplExecutionFailure)] {
        &self.failures
    }

    pub fn read_typed(
        &mut self,
        request: SrplReadRequest,
    ) -> Result<SrplReadResult<StructuredObject>, SrplExecutionFailure> {
        let environment = self.environment.clone();
        <Self as SrplTypedReadAdapter>::read_typed(self, request, &environment)
    }

    pub fn assert_typed(
        &mut self,
        request: SrplAssertRequest,
    ) -> Result<SrplAssertResult, SrplExecutionFailure> {
        let environment = self.environment.clone();
        <Self as SrplAssertionAdapter>::assert_typed(self, request, &environment)
    }

    pub fn update_typed(
        &mut self,
        request: SrplUpdateRequest,
    ) -> Result<SrplUpdateResult, SrplExecutionFailure> {
        let environment = self.environment.clone();
        <Self as SrplTypedUpdateAdapter>::update_typed(self, request, &environment)
    }

    pub fn emit_typed(
        &mut self,
        request: SrplEmitRequest,
    ) -> Result<SrplEmitResult, SrplExecutionFailure> {
        let environment = self.environment.clone();
        <Self as SrplTypedEmitAdapter>::emit_typed(self, request, &environment)
    }

    pub fn fail_typed(&mut self, request: SrplFailureRequest) -> Result<(), SrplExecutionFailure> {
        let environment = self.environment.clone();
        <Self as SrplFailureAdapter>::fail_typed(self, request, &environment)
    }
}

impl SrplTypedReadAdapter for SrplExecutionAdapter {
    type Row = StructuredObject;

    /// Implement typed table scans with predicate binding.
    /// Evaluates all predicates over the scoped environment before returning rows.
    fn read_typed(
        &mut self,
        request: SrplReadRequest,
        environment: &dyn SrplBindingEnvironment,
    ) -> Result<SrplReadResult<Self::Row>, SrplExecutionFailure> {
        request.validate().map_err(SrplExecutionFailure::from)?;

        // Mock: In production, this would perform a catalog table scan
        // with the predicates bound to the typed environment.
        // For now, return an empty result set.
        let rows = Vec::new();

        // Validate predicates can bind to environment
        for predicate in &request.predicates {
            self.evaluate_predicate_for_binding(predicate, environment)?;
        }

        // Create result and validate cardinality/bounds
        let result = SrplReadResult::new(rows, request.cardinality, request.row_bound)?;

        Ok(result)
    }
}

impl SrplAssertionAdapter for SrplExecutionAdapter {
    /// Evaluate predicates deterministically over the typed environment.
    fn assert_typed(
        &mut self,
        request: SrplAssertRequest,
        environment: &dyn SrplBindingEnvironment,
    ) -> Result<SrplAssertResult, SrplExecutionFailure> {
        request.validate().map_err(SrplExecutionFailure::from)?;

        // Evaluate predicate with binding
        let passed = self.evaluate_predicate_with_environment(&request.predicate, environment)?;

        Ok(SrplAssertResult::new(passed))
    }
}

impl SrplTypedUpdateAdapter for SrplExecutionAdapter {
    /// Implement in-transaction updates with all-or-nothing semantics.
    fn update_typed(
        &mut self,
        request: SrplUpdateRequest,
        environment: &dyn SrplBindingEnvironment,
    ) -> Result<SrplUpdateResult, SrplExecutionFailure> {
        request.validate().map_err(SrplExecutionFailure::from)?;

        // Check transaction is not aborted
        if self.transaction.is_aborted() {
            return Err(SrplExecutionFailure::SemanticViolation(
                "SRPL transaction aborted; cannot execute update".to_string(),
            ));
        }

        // Validate all predicates can bind to environment
        for predicate in &request.predicates {
            self.evaluate_predicate_for_binding(predicate, environment)?;
        }

        // Mock: In production, this would:
        // 1. Query the table with predicates
        // 2. Apply assignments to matching rows
        // 3. Record in transaction context
        // For now, return exact row count as requested
        let affected_rows = request.affected_rows.get();
        let result = SrplUpdateResult::new(affected_rows, request.affected_rows)?;

        Ok(result)
    }
}

impl SrplTypedEmitAdapter for SrplExecutionAdapter {
    /// Emit rows to result stream with backpressure handling.
    fn emit_typed(
        &mut self,
        request: SrplEmitRequest,
        environment: &dyn SrplBindingEnvironment,
    ) -> Result<SrplEmitResult, SrplExecutionFailure> {
        request.validate().map_err(SrplExecutionFailure::from)?;

        // Check backpressure constraint
        let row_count = request.row_bound.get();
        self.backpressure
            .buffer_rows(row_count as usize)
            .map_err(SrplExecutionFailure::from)?;

        // Mock: In production, this would emit rows to the result stream.
        // For now, validate bounds and release backpressure.
        let result = SrplEmitResult::new(row_count, request.cardinality, request.row_bound)?;

        // Release backpressure after emission
        self.backpressure.release_rows(row_count as usize);

        Ok(result)
    }
}

impl SrplFailureAdapter for SrplExecutionAdapter {
    /// Map SRPL execution failures deterministically.
    fn fail_typed(
        &mut self,
        request: SrplFailureRequest,
        environment: &dyn SrplBindingEnvironment,
    ) -> Result<(), SrplExecutionFailure> {
        request.validate().map_err(SrplExecutionFailure::from)?;

        // Record failure for recovery
        self.failures.push((request.code, request.failure.clone()));

        // Abort transaction on failure
        self.transaction.abort();

        Ok(())
    }
}

// ============================================================================
// PREDICATE EVALUATION
// ============================================================================

impl SrplExecutionAdapter {
    /// Evaluate a predicate to a boolean result using the provided environment.
    fn evaluate_predicate_with_environment(
        &self,
        predicate: &SrplPredicateIr,
        environment: &dyn SrplBindingEnvironment,
    ) -> Result<bool, SrplExecutionFailure> {
        match predicate {
            SrplPredicateIr::InputEqualsField {
                input,
                binding,
                field,
            } => {
                // Get input value
                let input_val = environment.get_input(input).ok_or_else(|| {
                    SrplExecutionFailure::SemanticViolation(format!(
                        "SRPL predicate input '{}' not bound",
                        input
                    ))
                })?;

                // Check if binding has any rows
                let row_count = environment.binding_row_count(binding).ok_or_else(|| {
                    SrplExecutionFailure::SemanticViolation(format!(
                        "SRPL predicate binding '{}' not found",
                        binding
                    ))
                })?;

                if row_count == 0 {
                    // No rows to compare; predicate is false
                    return Ok(false);
                }

                // Get field value from first row
                let field_val = environment
                    .get_field_from_binding(binding, 0, field)
                    .ok_or_else(|| {
                        SrplExecutionFailure::SemanticViolation(format!(
                            "SRPL predicate field '{}' not found in binding '{}'",
                            field, binding
                        ))
                    })?;

                Ok(input_val == field_val)
            }
            SrplPredicateIr::FieldGreaterThanOrEqualInput {
                binding,
                field,
                input,
            } => {
                // Get input value
                let input_val = environment.get_input(input).ok_or_else(|| {
                    SrplExecutionFailure::SemanticViolation(format!(
                        "SRPL predicate input '{}' not bound",
                        input
                    ))
                })?;

                // Check if binding has any rows
                let row_count = environment.binding_row_count(binding).ok_or_else(|| {
                    SrplExecutionFailure::SemanticViolation(format!(
                        "SRPL predicate binding '{}' not found",
                        binding
                    ))
                })?;

                if row_count == 0 {
                    // No rows to compare; predicate is false
                    return Ok(false);
                }

                // Get field value from first row
                let field_val = environment
                    .get_field_from_binding(binding, 0, field)
                    .ok_or_else(|| {
                        SrplExecutionFailure::SemanticViolation(format!(
                            "SRPL predicate field '{}' not found in binding '{}'",
                            field, binding
                        ))
                    })?;

                // Perform comparison
                match (&field_val, &input_val) {
                    (SrplBoundValue::Integer(f), SrplBoundValue::Integer(i)) => Ok(f >= i),
                    _ => Err(SrplExecutionFailure::SemanticViolation(
                        "SRPL comparison requires integer values".to_string(),
                    )),
                }
            }
        }
    }

    /// Check if a predicate can be evaluated (all bindings exist).
    fn evaluate_predicate_for_binding(
        &self,
        predicate: &SrplPredicateIr,
        environment: &dyn SrplBindingEnvironment,
    ) -> Result<(), SrplExecutionFailure> {
        match predicate {
            SrplPredicateIr::InputEqualsField {
                input,
                binding,
                field,
            }
            | SrplPredicateIr::FieldGreaterThanOrEqualInput {
                binding,
                field,
                input,
            } => {
                // Validate input exists
                if environment.get_input(input).is_none() {
                    return Err(SrplExecutionFailure::SemanticViolation(format!(
                        "SRPL predicate input '{}' not available",
                        input
                    )));
                }

                // Validate binding exists (would be populated by a prior read)
                // For reads, this check happens after binding is populated
                // For now, we allow bindings to not exist yet (they may be populated by earlier reads)
                Ok(())
            }
        }
    }

    /// Legacy method for backward compatibility during testing.
    /// Evaluates a predicate using the internal environment.
    fn evaluate_predicate(
        &self,
        predicate: &SrplPredicateIr,
    ) -> Result<bool, SrplExecutionFailure> {
        self.evaluate_predicate_with_environment(predicate, &self.environment)
    }

    /// Legacy method for backward compatibility during testing.
    /// Validates a predicate using the internal environment.
    fn evaluate_predicate_for_binding_legacy(
        &self,
        predicate: &SrplPredicateIr,
    ) -> Result<(), SrplExecutionFailure> {
        self.evaluate_predicate_for_binding(predicate, &self.environment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_srpl::execution_adapter::SrplBoundValue;

    #[test]
    fn structured_object_field_access() {
        let obj = StructuredObject::new()
            .with_field("id", FieldValue::Integer(42))
            .with_field("name", FieldValue::String("test".to_string()));

        assert_eq!(obj.get_field("id"), Some(&FieldValue::Integer(42)));
        assert_eq!(
            obj.get_field("name"),
            Some(&FieldValue::String("test".to_string()))
        );
        assert_eq!(obj.get_field("missing"), None);
    }

    #[test]
    fn field_value_conversions() {
        assert_eq!(FieldValue::Integer(42).as_i64(), Some(42));
        assert_eq!(
            FieldValue::String("hello".to_string()).as_string(),
            Some("hello")
        );
        assert_eq!(FieldValue::Bool(true).as_bool(), Some(true));

        assert_eq!(FieldValue::String("x".to_string()).as_i64(), None);
    }

    #[test]
    fn srpl_typed_environment_inputs() {
        let mut env = SrplTypedEnvironment::new();
        env.add_input("param1", FieldValue::Integer(100));
        env.add_input("param2", FieldValue::String("data".to_string()));

        assert_eq!(
            env.get_input_internal("param1"),
            Some(&FieldValue::Integer(100))
        );
        assert_eq!(
            env.get_input_internal("param2"),
            Some(&FieldValue::String("data".to_string()))
        );
        assert_eq!(env.get_input_internal("missing"), None);
    }

    #[test]
    fn srpl_typed_environment_bindings() {
        let mut env = SrplTypedEnvironment::new();
        let rows = vec![
            StructuredObject::new().with_field("id", FieldValue::Integer(1)),
            StructuredObject::new().with_field("id", FieldValue::Integer(2)),
        ];
        env.bind_read("users", rows.clone());

        assert_eq!(env.get_binding_internal("users"), Some(&rows));
        assert_eq!(env.get_binding_internal("missing"), None);
    }

    #[test]
    fn srpl_transaction_context_records_updates() {
        let mut tx = SrplTransactionContext::new();
        let obj =
            StructuredObject::new().with_field("status", FieldValue::String("active".to_string()));

        assert!(tx.record_update("users", 0, obj.clone()).is_ok());
        assert_eq!(tx.pending_updates().len(), 1);
    }

    #[test]
    fn srpl_transaction_context_abort_prevents_updates() {
        let mut tx = SrplTransactionContext::new();
        tx.abort();

        let obj = StructuredObject::new();
        let result = tx.record_update("users", 0, obj);
        assert!(result.is_err());
    }

    #[test]
    fn srpl_backpressure_respects_limits() {
        let mut bp = SrplStreamBackpressure::new(100);

        assert!(bp.buffer_rows(50).is_ok());
        assert!(bp.buffer_rows(50).is_ok());
        assert!(bp.buffer_rows(1).is_err());
    }

    #[test]
    fn srpl_backpressure_release() {
        let mut bp = SrplStreamBackpressure::new(100);
        bp.buffer_rows(80).ok();
        bp.release_rows(30);
        assert!(bp.buffer_rows(50).is_ok());
    }

    #[test]
    fn srpl_execution_adapter_creation() {
        let adapter = SrplExecutionAdapter::new(1000);
        assert!(!adapter.is_transaction_aborted());
        assert_eq!(adapter.failures().len(), 0);
    }

    #[test]
    fn srpl_execution_adapter_abort_transaction() {
        let mut adapter = SrplExecutionAdapter::new(1000);
        assert!(!adapter.is_transaction_aborted());
        adapter.abort_transaction();
        assert!(adapter.is_transaction_aborted());
    }

    #[test]
    fn srpl_predicate_evaluation_input_equals_field() {
        let mut adapter = SrplExecutionAdapter::new(1000);
        adapter.add_input("id", FieldValue::Integer(42));

        let row = StructuredObject::new().with_field("user_id", FieldValue::Integer(42));
        adapter.environment_mut().bind_read("users", vec![row]);

        let predicate = SrplPredicateIr::InputEqualsField {
            input: "id".to_string(),
            binding: "users".to_string(),
            field: "user_id".to_string(),
        };

        let result = adapter.evaluate_predicate(&predicate);
        assert_eq!(result, Ok(true));
    }

    #[test]
    fn srpl_predicate_evaluation_field_gte_input() {
        let mut adapter = SrplExecutionAdapter::new(1000);
        adapter.add_input("threshold", FieldValue::Integer(10));

        let row = StructuredObject::new().with_field("quantity", FieldValue::Integer(15));
        adapter.environment_mut().bind_read("items", vec![row]);

        let predicate = SrplPredicateIr::FieldGreaterThanOrEqualInput {
            binding: "items".to_string(),
            field: "quantity".to_string(),
            input: "threshold".to_string(),
        };

        let result = adapter.evaluate_predicate(&predicate);
        assert_eq!(result, Ok(true));
    }

    #[test]
    fn srpl_predicate_evaluation_missing_input() {
        let adapter = SrplExecutionAdapter::new(1000);

        let predicate = SrplPredicateIr::InputEqualsField {
            input: "missing_id".to_string(),
            binding: "users".to_string(),
            field: "user_id".to_string(),
        };

        let result = adapter.evaluate_predicate(&predicate);
        assert!(matches!(
            result,
            Err(SrplExecutionFailure::SemanticViolation(_))
        ));
    }

    #[test]
    fn srpl_predicate_evaluation_missing_binding() {
        let mut adapter = SrplExecutionAdapter::new(1000);
        adapter.add_input("id", FieldValue::Integer(42));

        let predicate = SrplPredicateIr::InputEqualsField {
            input: "id".to_string(),
            binding: "missing_binding".to_string(),
            field: "user_id".to_string(),
        };

        let result = adapter.evaluate_predicate(&predicate);
        assert!(matches!(
            result,
            Err(SrplExecutionFailure::SemanticViolation(_))
        ));
    }

    // ========================================================================
    // NEW: Value Binding Environment Tests
    // ========================================================================

    #[test]
    fn binding_environment_trait_get_input() {
        let mut env = SrplTypedEnvironment::new();
        env.add_input("user_id", FieldValue::Integer(42));
        env.add_input("name", FieldValue::String("Alice".to_string()));

        // Using the trait interface
        let binding: &dyn SrplBindingEnvironment = &env;
        assert_eq!(
            binding.get_input("user_id"),
            Some(SrplBoundValue::Integer(42))
        );
        assert_eq!(
            binding.get_input("name"),
            Some(SrplBoundValue::String("Alice".to_string()))
        );
        assert_eq!(binding.get_input("missing"), None);
    }

    #[test]
    fn binding_environment_trait_get_field_from_binding() {
        let mut env = SrplTypedEnvironment::new();
        let row1 = StructuredObject::new()
            .with_field("id", FieldValue::Integer(1))
            .with_field("status", FieldValue::String("active".to_string()));
        let row2 = StructuredObject::new()
            .with_field("id", FieldValue::Integer(2))
            .with_field("status", FieldValue::String("inactive".to_string()));
        env.bind_read("users", vec![row1, row2]);

        let binding: &dyn SrplBindingEnvironment = &env;

        // Access first row
        assert_eq!(
            binding.get_field_from_binding("users", 0, "id"),
            Some(SrplBoundValue::Integer(1))
        );
        assert_eq!(
            binding.get_field_from_binding("users", 0, "status"),
            Some(SrplBoundValue::String("active".to_string()))
        );

        // Access second row
        assert_eq!(
            binding.get_field_from_binding("users", 1, "id"),
            Some(SrplBoundValue::Integer(2))
        );

        // Missing row index
        assert_eq!(binding.get_field_from_binding("users", 2, "id"), None);

        // Missing binding
        assert_eq!(binding.get_field_from_binding("missing", 0, "id"), None);

        // Missing field
        assert_eq!(
            binding.get_field_from_binding("users", 0, "missing_field"),
            None
        );
    }

    #[test]
    fn binding_environment_trait_binding_row_count() {
        let mut env = SrplTypedEnvironment::new();
        env.bind_read("empty", vec![]);
        env.bind_read(
            "single",
            vec![StructuredObject::new().with_field("x", FieldValue::Integer(1))],
        );
        env.bind_read(
            "multiple",
            vec![
                StructuredObject::new().with_field("x", FieldValue::Integer(1)),
                StructuredObject::new().with_field("x", FieldValue::Integer(2)),
                StructuredObject::new().with_field("x", FieldValue::Integer(3)),
            ],
        );

        let binding: &dyn SrplBindingEnvironment = &env;

        assert_eq!(binding.binding_row_count("empty"), Some(0));
        assert_eq!(binding.binding_row_count("single"), Some(1));
        assert_eq!(binding.binding_row_count("multiple"), Some(3));
        assert_eq!(binding.binding_row_count("missing"), None);
    }

    #[test]
    fn predicate_evaluation_with_environment_input_equals_field() {
        let mut adapter = SrplExecutionAdapter::new(1000);
        let mut env = SrplTypedEnvironment::new();

        env.add_input("order_id", FieldValue::Integer(42));
        let row = StructuredObject::new().with_field("id", FieldValue::Integer(42));
        env.bind_read("orders", vec![row]);

        let predicate = SrplPredicateIr::InputEqualsField {
            input: "order_id".to_string(),
            binding: "orders".to_string(),
            field: "id".to_string(),
        };

        let result = adapter.evaluate_predicate_with_environment(&predicate, &env);
        assert_eq!(result, Ok(true));
    }

    #[test]
    fn predicate_evaluation_with_environment_input_not_equals_field() {
        let mut adapter = SrplExecutionAdapter::new(1000);
        let mut env = SrplTypedEnvironment::new();

        env.add_input("order_id", FieldValue::Integer(42));
        let row = StructuredObject::new().with_field("id", FieldValue::Integer(99));
        env.bind_read("orders", vec![row]);

        let predicate = SrplPredicateIr::InputEqualsField {
            input: "order_id".to_string(),
            binding: "orders".to_string(),
            field: "id".to_string(),
        };

        let result = adapter.evaluate_predicate_with_environment(&predicate, &env);
        assert_eq!(result, Ok(false));
    }

    #[test]
    fn predicate_evaluation_with_environment_field_gte_input() {
        let mut adapter = SrplExecutionAdapter::new(1000);
        let mut env = SrplTypedEnvironment::new();

        env.add_input("min_stock", FieldValue::Integer(10));
        let row = StructuredObject::new().with_field("available", FieldValue::Integer(15));
        env.bind_read("inventory", vec![row]);

        let predicate = SrplPredicateIr::FieldGreaterThanOrEqualInput {
            binding: "inventory".to_string(),
            field: "available".to_string(),
            input: "min_stock".to_string(),
        };

        let result = adapter.evaluate_predicate_with_environment(&predicate, &env);
        assert_eq!(result, Ok(true));
    }

    #[test]
    fn predicate_evaluation_with_environment_field_not_gte_input() {
        let mut adapter = SrplExecutionAdapter::new(1000);
        let mut env = SrplTypedEnvironment::new();

        env.add_input("min_stock", FieldValue::Integer(20));
        let row = StructuredObject::new().with_field("available", FieldValue::Integer(15));
        env.bind_read("inventory", vec![row]);

        let predicate = SrplPredicateIr::FieldGreaterThanOrEqualInput {
            binding: "inventory".to_string(),
            field: "available".to_string(),
            input: "min_stock".to_string(),
        };

        let result = adapter.evaluate_predicate_with_environment(&predicate, &env);
        assert_eq!(result, Ok(false));
    }

    #[test]
    fn predicate_evaluation_empty_binding_returns_false() {
        let mut adapter = SrplExecutionAdapter::new(1000);
        let mut env = SrplTypedEnvironment::new();

        env.add_input("id", FieldValue::Integer(42));
        env.bind_read("users", vec![]); // Empty binding

        let predicate = SrplPredicateIr::InputEqualsField {
            input: "id".to_string(),
            binding: "users".to_string(),
            field: "user_id".to_string(),
        };

        let result = adapter.evaluate_predicate_with_environment(&predicate, &env);
        assert_eq!(result, Ok(false));
    }

    #[test]
    fn predicate_evaluation_with_environment_missing_input_error() {
        let adapter = SrplExecutionAdapter::new(1000);
        let mut env = SrplTypedEnvironment::new();

        let row = StructuredObject::new().with_field("id", FieldValue::Integer(42));
        env.bind_read("users", vec![row]);

        let predicate = SrplPredicateIr::InputEqualsField {
            input: "missing_input".to_string(),
            binding: "users".to_string(),
            field: "id".to_string(),
        };

        let result = adapter.evaluate_predicate_with_environment(&predicate, &env);
        assert!(
            matches!(result, Err(SrplExecutionFailure::SemanticViolation(msg)) if msg.contains("missing_input"))
        );
    }

    #[test]
    fn predicate_evaluation_with_environment_missing_binding_error() {
        let adapter = SrplExecutionAdapter::new(1000);
        let mut env = SrplTypedEnvironment::new();

        env.add_input("id", FieldValue::Integer(42));

        let predicate = SrplPredicateIr::InputEqualsField {
            input: "id".to_string(),
            binding: "missing_binding".to_string(),
            field: "id".to_string(),
        };

        let result = adapter.evaluate_predicate_with_environment(&predicate, &env);
        assert!(
            matches!(result, Err(SrplExecutionFailure::SemanticViolation(msg)) if msg.contains("missing_binding"))
        );
    }

    #[test]
    fn predicate_evaluation_with_environment_missing_field_error() {
        let adapter = SrplExecutionAdapter::new(1000);
        let mut env = SrplTypedEnvironment::new();

        env.add_input("id", FieldValue::Integer(42));
        let row = StructuredObject::new().with_field("other_field", FieldValue::Integer(42));
        env.bind_read("users", vec![row]);

        let predicate = SrplPredicateIr::InputEqualsField {
            input: "id".to_string(),
            binding: "users".to_string(),
            field: "missing_field".to_string(),
        };

        let result = adapter.evaluate_predicate_with_environment(&predicate, &env);
        assert!(
            matches!(result, Err(SrplExecutionFailure::SemanticViolation(msg)) if msg.contains("missing_field"))
        );
    }

    #[test]
    fn predicate_evaluation_with_environment_type_mismatch_error() {
        let adapter = SrplExecutionAdapter::new(1000);
        let mut env = SrplTypedEnvironment::new();

        env.add_input("text", FieldValue::String("hello".to_string()));
        let row =
            StructuredObject::new().with_field("value", FieldValue::String("world".to_string()));
        env.bind_read("data", vec![row]);

        let predicate = SrplPredicateIr::FieldGreaterThanOrEqualInput {
            binding: "data".to_string(),
            field: "value".to_string(),
            input: "text".to_string(),
        };

        let result = adapter.evaluate_predicate_with_environment(&predicate, &env);
        assert!(
            matches!(result, Err(SrplExecutionFailure::SemanticViolation(msg)) if msg.contains("integer"))
        );
    }

    #[test]
    fn predicate_validation_with_environment() {
        let adapter = SrplExecutionAdapter::new(1000);
        let mut env = SrplTypedEnvironment::new();

        env.add_input("id", FieldValue::Integer(42));

        let predicate = SrplPredicateIr::InputEqualsField {
            input: "id".to_string(),
            binding: "users".to_string(),
            field: "id".to_string(),
        };

        // Validation should pass (binding may not exist yet, will be populated by read)
        let result = adapter.evaluate_predicate_for_binding(&predicate, &env);
        assert!(result.is_ok());
    }

    #[test]
    fn predicate_validation_with_environment_missing_input() {
        let adapter = SrplExecutionAdapter::new(1000);
        let env = SrplTypedEnvironment::new();

        let predicate = SrplPredicateIr::InputEqualsField {
            input: "missing".to_string(),
            binding: "users".to_string(),
            field: "id".to_string(),
        };

        // Validation should fail (input does not exist)
        let result = adapter.evaluate_predicate_for_binding(&predicate, &env);
        assert!(
            matches!(result, Err(SrplExecutionFailure::SemanticViolation(msg)) if msg.contains("missing"))
        );
    }

    #[test]
    fn multiple_inputs_and_bindings_in_environment() {
        let mut adapter = SrplExecutionAdapter::new(1000);
        let mut env = SrplTypedEnvironment::new();

        // Set up multiple inputs
        env.add_input("user_id", FieldValue::Integer(1));
        env.add_input("order_id", FieldValue::Integer(100));
        env.add_input("amount_threshold", FieldValue::Integer(50));

        // Set up multiple bindings
        env.bind_read(
            "users",
            vec![
                StructuredObject::new()
                    .with_field("id", FieldValue::Integer(1))
                    .with_field("name", FieldValue::String("Alice".to_string())),
            ],
        );
        env.bind_read(
            "orders",
            vec![
                StructuredObject::new()
                    .with_field("id", FieldValue::Integer(100))
                    .with_field("amount", FieldValue::Integer(75)),
            ],
        );

        // Check that all bindings can be resolved
        let pred1 = SrplPredicateIr::InputEqualsField {
            input: "user_id".to_string(),
            binding: "users".to_string(),
            field: "id".to_string(),
        };

        let pred2 = SrplPredicateIr::InputEqualsField {
            input: "order_id".to_string(),
            binding: "orders".to_string(),
            field: "id".to_string(),
        };

        let pred3 = SrplPredicateIr::FieldGreaterThanOrEqualInput {
            binding: "orders".to_string(),
            field: "amount".to_string(),
            input: "amount_threshold".to_string(),
        };

        assert_eq!(
            adapter.evaluate_predicate_with_environment(&pred1, &env),
            Ok(true)
        );
        assert_eq!(
            adapter.evaluate_predicate_with_environment(&pred2, &env),
            Ok(true)
        );
        assert_eq!(
            adapter.evaluate_predicate_with_environment(&pred3, &env),
            Ok(true)
        );
    }

    #[test]
    fn field_value_conversion_to_srpl_bound_value() {
        assert_eq!(
            FieldValue::Integer(42).to_srpl_bound_value(),
            SrplBoundValue::Integer(42)
        );
        assert_eq!(
            FieldValue::String("test".to_string()).to_srpl_bound_value(),
            SrplBoundValue::String("test".to_string())
        );
        assert_eq!(
            FieldValue::Bool(true).to_srpl_bound_value(),
            SrplBoundValue::Bool(true)
        );
        assert_eq!(FieldValue::Null.to_srpl_bound_value(), SrplBoundValue::Null);
    }

    #[test]
    fn field_value_roundtrip_conversion() {
        let original = FieldValue::Integer(42);
        let bound_value = original.to_srpl_bound_value();
        let reconstructed = FieldValue::from_srpl_bound_value(bound_value);
        assert_eq!(original, reconstructed);
    }
}
