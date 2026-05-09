use andromeda_contract::CatalogObjectRef;
use andromeda_srpl_execution_adapter::{
    SrplAssertRequest, SrplAssertResult, SrplAssertionAdapter, SrplEmitRequest, SrplEmitResult,
    SrplExecutionFailure, SrplFailureAdapter, SrplFailureRequest, SrplReadRequest, SrplReadResult,
    SrplRowBound, SrplTypedEmitAdapter, SrplTypedReadAdapter, SrplTypedUpdateAdapter,
    SrplUpdateRequest, SrplUpdateResult,
};
use andromeda_srpl_ir::{
    BoundSrplOperationPlan, Cardinality, ExecutableProcedurePlan, SrplAssignmentIr,
    SrplEmitValueIr, SrplPredicateIr,
};

use super::{
    diagnostics::{assertion_failed, operation_context, raise_invoked, unbounded_update_contract},
    execution_state::{SrplExecutionState, SrplInterpreterReport},
    expression::row_bound_for_read,
};

pub(super) struct SrplOperationExecutor<'plan, 'adapter, Adapter> {
    plan: &'plan ExecutableProcedurePlan,
    adapter: &'adapter mut Adapter,
    state: SrplExecutionState,
}

impl<'plan, 'adapter, Adapter> SrplOperationExecutor<'plan, 'adapter, Adapter>
where
    Adapter: SrplTypedReadAdapter
        + SrplAssertionAdapter
        + SrplTypedUpdateAdapter
        + SrplTypedEmitAdapter
        + SrplFailureAdapter,
{
    pub(super) fn new(
        plan: &'plan ExecutableProcedurePlan,
        adapter: &'adapter mut Adapter,
    ) -> Self {
        Self {
            plan,
            adapter,
            state: SrplExecutionState::new(),
        }
    }

    pub(super) fn execute_plan(mut self) -> Result<SrplInterpreterReport, SrplExecutionFailure> {
        for operation in &self.plan.body.operations {
            self.execute_operation(operation)?;
            self.state.record_operation();
        }

        Ok(self.state.finish())
    }

    fn execute_operation(
        &mut self,
        operation: &BoundSrplOperationPlan,
    ) -> Result<(), SrplExecutionFailure> {
        match operation {
            BoundSrplOperationPlan::ReadTable {
                ordinal,
                source,
                cardinality,
                predicates,
                ..
            } => self.execute_read(*ordinal, source, *cardinality, predicates),
            BoundSrplOperationPlan::Assert {
                ordinal,
                predicate,
                failure_code,
            } => self.execute_assert(*ordinal, predicate, failure_code),
            BoundSrplOperationPlan::UpdateTable {
                ordinal,
                target,
                predicates,
                assignments,
                affected_rows_exact,
            } => self.execute_update(
                *ordinal,
                target,
                predicates,
                assignments,
                *affected_rows_exact,
            ),
            BoundSrplOperationPlan::Emit {
                ordinal,
                stream,
                values,
            } => self.execute_emit(*ordinal, stream, values),
            BoundSrplOperationPlan::Raise { ordinal, code } => self.execute_raise(*ordinal, code),
        }
    }

    fn execute_read(
        &mut self,
        ordinal: u32,
        source: &CatalogObjectRef,
        cardinality: Cardinality,
        predicates: &[SrplPredicateIr],
    ) -> Result<(), SrplExecutionFailure> {
        let bound = row_bound_for_read(cardinality).map_err(SrplExecutionFailure::from)?;
        let request = SrplReadRequest::new(
            operation_context(self.plan, ordinal)?,
            source.clone(),
            cardinality,
            bound,
            predicates.to_vec(),
        )
        .map_err(SrplExecutionFailure::from)?;
        let result = self.adapter.read_typed(request, self.state.environment())?;
        SrplReadResult::new(result.rows, cardinality, bound)?;
        self.state.record_read();
        Ok(())
    }

    fn execute_assert(
        &mut self,
        ordinal: u32,
        predicate: &SrplPredicateIr,
        failure_code: &str,
    ) -> Result<(), SrplExecutionFailure> {
        let request = SrplAssertRequest::new(
            operation_context(self.plan, ordinal)?,
            predicate.clone(),
            failure_code,
        )
        .map_err(SrplExecutionFailure::from)?;
        let SrplAssertResult { passed } = self
            .adapter
            .assert_typed(request, self.state.environment())?;
        self.state.record_assertion();
        if !passed {
            let failure = assertion_failed(failure_code);
            self.record_failure(ordinal, failure_code, failure.clone())?;
            return Err(failure);
        }
        Ok(())
    }

    fn execute_update(
        &mut self,
        ordinal: u32,
        target: &CatalogObjectRef,
        predicates: &[SrplPredicateIr],
        assignments: &[SrplAssignmentIr],
        affected_rows_exact: Option<u64>,
    ) -> Result<(), SrplExecutionFailure> {
        let bound = SrplRowBound::exact(affected_rows_exact.ok_or_else(unbounded_update_contract)?)
            .map_err(SrplExecutionFailure::from)?;
        let request = SrplUpdateRequest::new(
            operation_context(self.plan, ordinal)?,
            target.clone(),
            bound,
            predicates.to_vec(),
            assignments.to_vec(),
        )
        .map_err(SrplExecutionFailure::from)?;
        let result = self
            .adapter
            .update_typed(request, self.state.environment())?;
        SrplUpdateResult::new(result.affected_rows, bound)?;
        self.state.record_update();
        Ok(())
    }

    fn execute_emit(
        &mut self,
        ordinal: u32,
        stream: &str,
        values: &[SrplEmitValueIr],
    ) -> Result<(), SrplExecutionFailure> {
        let bound = SrplRowBound::exact(1).map_err(SrplExecutionFailure::from)?;
        let request = SrplEmitRequest::new(
            operation_context(self.plan, ordinal)?,
            stream,
            Cardinality::One,
            bound,
            values.to_vec(),
        )
        .map_err(SrplExecutionFailure::from)?;
        let result = self.adapter.emit_typed(request, self.state.environment())?;
        SrplEmitResult::new(result.emitted_rows, Cardinality::One, bound)?;
        self.state.record_emit();
        Ok(())
    }

    fn execute_raise(&mut self, ordinal: u32, code: &str) -> Result<(), SrplExecutionFailure> {
        let failure = raise_invoked(code);
        self.record_failure(ordinal, code, failure.clone())?;
        Err(failure)
    }

    fn record_failure(
        &mut self,
        ordinal: u32,
        code: &str,
        failure: SrplExecutionFailure,
    ) -> Result<(), SrplExecutionFailure> {
        self.adapter.fail_typed(
            SrplFailureRequest::new(operation_context(self.plan, ordinal)?, code, failure)
                .map_err(SrplExecutionFailure::from)?,
            self.state.environment(),
        )
    }
}
