use andromeda_srpl::{
    execution_adapter::{
        SrplAssertRequest, SrplAssertResult, SrplAssertionAdapter, SrplBindingEnvironment,
        SrplBoundValue, SrplEmitRequest, SrplEmitResult, SrplExecutionFailure, SrplFailureAdapter,
        SrplFailureRequest, SrplReadRequest, SrplReadResult, SrplTypedEmitAdapter,
        SrplTypedReadAdapter, SrplTypedUpdateAdapter, SrplUpdateRequest, SrplUpdateResult,
    },
    procedure_model::SrplPredicateIr,
};

use super::{
    backpressure::SrplStreamBackpressure,
    environment::SrplTypedEnvironment,
    transaction_context::SrplTransactionContext,
    values::{FieldValue, StructuredObject},
};

/// Complete SRPL execution adapter.
pub struct SrplExecutionAdapter {
    environment: SrplTypedEnvironment,
    transaction: SrplTransactionContext,
    backpressure: SrplStreamBackpressure,
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

    pub fn add_input(&mut self, name: impl Into<String>, value: FieldValue) {
        self.environment.add_input(name, value);
    }

    pub fn environment_mut(&mut self) -> &mut SrplTypedEnvironment {
        &mut self.environment
    }

    pub fn transaction(&self) -> &SrplTransactionContext {
        &self.transaction
    }

    pub fn abort_transaction(&mut self) {
        self.transaction.abort();
    }

    pub fn is_transaction_aborted(&self) -> bool {
        self.transaction.is_aborted()
    }

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

    fn read_typed(
        &mut self,
        request: SrplReadRequest,
        environment: &dyn SrplBindingEnvironment,
    ) -> Result<SrplReadResult<Self::Row>, SrplExecutionFailure> {
        request.validate().map_err(SrplExecutionFailure::from)?;

        // This adapter validates predicate binding; durable catalog scanning is
        // owned by the runtime layer above this SRPL boundary.
        let rows = Vec::new();

        for predicate in &request.predicates {
            self.evaluate_predicate_for_binding(predicate, environment)?;
        }

        let result = SrplReadResult::new(rows, request.cardinality, request.row_bound)?;

        Ok(result)
    }
}

impl SrplAssertionAdapter for SrplExecutionAdapter {
    fn assert_typed(
        &mut self,
        request: SrplAssertRequest,
        environment: &dyn SrplBindingEnvironment,
    ) -> Result<SrplAssertResult, SrplExecutionFailure> {
        request.validate().map_err(SrplExecutionFailure::from)?;

        let passed = self.evaluate_predicate_with_environment(&request.predicate, environment)?;

        Ok(SrplAssertResult::new(passed))
    }
}

impl SrplTypedUpdateAdapter for SrplExecutionAdapter {
    fn update_typed(
        &mut self,
        request: SrplUpdateRequest,
        environment: &dyn SrplBindingEnvironment,
    ) -> Result<SrplUpdateResult, SrplExecutionFailure> {
        request.validate().map_err(SrplExecutionFailure::from)?;

        if self.transaction.is_aborted() {
            return Err(SrplExecutionFailure::SemanticViolation(
                "SRPL transaction aborted; cannot execute update".to_string(),
            ));
        }

        for predicate in &request.predicates {
            self.evaluate_predicate_for_binding(predicate, environment)?;
        }

        // Runtime storage owns physical row mutation; this boundary preserves
        // the typed affected-row contract for deterministic SRPL execution.
        let affected_rows = request.affected_rows.get();
        let result = SrplUpdateResult::new(affected_rows, request.affected_rows)?;

        Ok(result)
    }
}

impl SrplTypedEmitAdapter for SrplExecutionAdapter {
    fn emit_typed(
        &mut self,
        request: SrplEmitRequest,
        _environment: &dyn SrplBindingEnvironment,
    ) -> Result<SrplEmitResult, SrplExecutionFailure> {
        request.validate().map_err(SrplExecutionFailure::from)?;

        let row_count = request.row_bound.get();
        self.backpressure
            .buffer_rows(row_count as usize)
            .map_err(SrplExecutionFailure::from)?;

        // The runtime result stream owns payload emission; this boundary
        // validates row bounds and accounts for backpressure.
        let result = SrplEmitResult::new(row_count, request.cardinality, request.row_bound)?;

        self.backpressure.release_rows(row_count as usize);

        Ok(result)
    }
}

impl SrplFailureAdapter for SrplExecutionAdapter {
    fn fail_typed(
        &mut self,
        request: SrplFailureRequest,
        _environment: &dyn SrplBindingEnvironment,
    ) -> Result<(), SrplExecutionFailure> {
        request.validate().map_err(SrplExecutionFailure::from)?;

        self.failures.push((request.code, request.failure.clone()));

        self.transaction.abort();

        Ok(())
    }
}

impl SrplExecutionAdapter {
    /// Evaluate a predicate to a boolean result using the provided environment.
    pub(super) fn evaluate_predicate_with_environment(
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
                let input_val = environment.get_input(input).ok_or_else(|| {
                    SrplExecutionFailure::SemanticViolation(format!(
                        "SRPL predicate input '{}' not bound",
                        input
                    ))
                })?;

                let row_count = environment.binding_row_count(binding).ok_or_else(|| {
                    SrplExecutionFailure::SemanticViolation(format!(
                        "SRPL predicate binding '{}' not found",
                        binding
                    ))
                })?;

                if row_count == 0 {
                    return Ok(false);
                }

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
                let input_val = environment.get_input(input).ok_or_else(|| {
                    SrplExecutionFailure::SemanticViolation(format!(
                        "SRPL predicate input '{}' not bound",
                        input
                    ))
                })?;

                let row_count = environment.binding_row_count(binding).ok_or_else(|| {
                    SrplExecutionFailure::SemanticViolation(format!(
                        "SRPL predicate binding '{}' not found",
                        binding
                    ))
                })?;

                if row_count == 0 {
                    return Ok(false);
                }

                let field_val = environment
                    .get_field_from_binding(binding, 0, field)
                    .ok_or_else(|| {
                        SrplExecutionFailure::SemanticViolation(format!(
                            "SRPL predicate field '{}' not found in binding '{}'",
                            field, binding
                        ))
                    })?;

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
    pub(super) fn evaluate_predicate_for_binding(
        &self,
        predicate: &SrplPredicateIr,
        environment: &dyn SrplBindingEnvironment,
    ) -> Result<(), SrplExecutionFailure> {
        match predicate {
            SrplPredicateIr::InputEqualsField {
                input,
                binding: _,
                field: _,
            }
            | SrplPredicateIr::FieldGreaterThanOrEqualInput {
                binding: _,
                field: _,
                input,
            } => {
                if environment.get_input(input).is_none() {
                    return Err(SrplExecutionFailure::SemanticViolation(format!(
                        "SRPL predicate input '{}' not available",
                        input
                    )));
                }

                // Read target bindings may be materialized after input validation.
                Ok(())
            }
        }
    }

    /// Evaluate a predicate using the adapter-owned environment.
    #[cfg(test)]
    pub(super) fn evaluate_predicate(
        &self,
        predicate: &SrplPredicateIr,
    ) -> Result<bool, SrplExecutionFailure> {
        self.evaluate_predicate_with_environment(predicate, &self.environment)
    }
}
