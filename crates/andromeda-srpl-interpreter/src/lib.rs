#![forbid(unsafe_code)]

//! Deterministic SRPL IR interpreter.
//!
//! This module executes the currently bound SRPL operation set:
//! `ReadTable`, `Assert`, `UpdateTable`, `Emit`, and `Raise`. The interpreter
//! performs a complete validation pass before invoking any adapter method, then
//! executes operations in dense ordinal/source order and fails fast. There is no
//! storage, transport, transaction, physical execution, ad hoc SQL, or
//! runtime-dispatch dependency in this module; all external behavior is behind
//! the typed adapter traits from `andromeda-srpl-execution-adapter`.

mod diagnostics;
mod execution_state;
mod expression;
mod identifier;
mod operation;
mod validation;

use andromeda_error::AndromedaResult;
use andromeda_srpl_execution_adapter::{
    SrplAssertionAdapter, SrplExecutionFailure, SrplFailureAdapter, SrplTypedEmitAdapter,
    SrplTypedReadAdapter, SrplTypedUpdateAdapter,
};
use andromeda_srpl_ir::ExecutableProcedurePlan;

pub use execution_state::SrplInterpreterReport;

use operation::SrplOperationExecutor;

/// Stateless interpreter for a catalog-bound executable SRPL plan.
pub struct SrplIrInterpreter;

impl SrplIrInterpreter {
    /// Validates the whole plan before adapter side effects are possible.
    pub fn validate_plan(plan: &ExecutableProcedurePlan) -> AndromedaResult<()> {
        validation::validate_plan(plan)
    }

    /// Executes a validated plan over typed adapters in ordinal/source order.
    pub fn execute<Row, Adapter>(
        plan: &ExecutableProcedurePlan,
        adapter: &mut Adapter,
    ) -> Result<SrplInterpreterReport, SrplExecutionFailure>
    where
        Adapter: SrplTypedReadAdapter<Row = Row>
            + SrplAssertionAdapter
            + SrplTypedUpdateAdapter
            + SrplTypedEmitAdapter
            + SrplFailureAdapter,
    {
        Self::validate_plan(plan).map_err(SrplExecutionFailure::from)?;

        SrplOperationExecutor::new(plan, adapter).execute_plan()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_srpl_execution_adapter::SrplExecutionFailure;
    use andromeda_srpl_ir::{BoundSrplOperationPlan, Cardinality};
    use andromeda_srpl_test_fixtures::{
        FakeSrplAdapter as FakeAdapter, srpl_assignment as assignment,
        srpl_emit_value as emit_value, srpl_happy_path_plan as happy_path_plan, srpl_plan as plan,
        srpl_predicate as predicate, srpl_stock_object as stock_object,
    };

    #[test]
    fn interpreter_executes_supported_ir_in_ordinal_order() {
        let plan = happy_path_plan();
        let mut adapter = FakeAdapter::passing();

        let report = SrplIrInterpreter::execute(&plan, &mut adapter).unwrap();

        assert_eq!(
            adapter.events,
            vec!["read:0", "assert:1", "update:2", "emit:3"]
        );
        assert_eq!(report.operations_executed, 4);
        assert_eq!(report.reads, 1);
        assert_eq!(report.assertions, 1);
        assert_eq!(report.updates, 1);
        assert_eq!(report.emits, 1);
    }

    #[test]
    fn interpreter_rejects_unbounded_read_before_side_effects() {
        let plan = plan(vec![BoundSrplOperationPlan::ReadTable {
            ordinal: 0,
            source: stock_object(),
            binding: "Stock".to_string(),
            cardinality: Cardinality::Many,
            predicates: vec![predicate()],
        }]);
        let mut adapter = FakeAdapter::passing();

        let error = SrplIrInterpreter::execute(&plan, &mut adapter).unwrap_err();

        assert!(matches!(error, SrplExecutionFailure::SemanticViolation(_)));
        assert!(adapter.events.is_empty());
    }

    #[test]
    fn interpreter_rejects_unbounded_update_before_side_effects() {
        let plan = plan(vec![BoundSrplOperationPlan::UpdateTable {
            ordinal: 0,
            target: stock_object(),
            predicates: vec![predicate()],
            assignments: vec![assignment()],
            affected_rows_exact: None,
        }]);
        let mut adapter = FakeAdapter::passing();

        let error = SrplIrInterpreter::execute(&plan, &mut adapter).unwrap_err();

        assert!(matches!(error, SrplExecutionFailure::SemanticViolation(_)));
        assert!(adapter.events.is_empty());
    }

    #[test]
    fn interpreter_rejects_missing_emit_values_before_side_effects() {
        let plan = plan(vec![BoundSrplOperationPlan::Emit {
            ordinal: 0,
            stream: "Reservation".to_string(),
            values: Vec::new(),
        }]);
        let mut adapter = FakeAdapter::passing();

        let error = SrplIrInterpreter::execute(&plan, &mut adapter).unwrap_err();

        assert!(matches!(error, SrplExecutionFailure::SemanticViolation(_)));
        assert!(adapter.events.is_empty());
    }

    #[test]
    fn interpreter_rejects_contract_evidence_mismatch_before_side_effects() {
        let mut plan = plan(vec![BoundSrplOperationPlan::ReadTable {
            ordinal: 0,
            source: stock_object(),
            binding: "Stock".to_string(),
            cardinality: Cardinality::One,
            predicates: vec![predicate()],
        }]);
        plan.evidence.bound_objects.clear();
        let mut adapter = FakeAdapter::passing();

        let error = SrplIrInterpreter::execute(&plan, &mut adapter).unwrap_err();

        assert!(matches!(error, SrplExecutionFailure::ContractViolation(_)));
        assert!(adapter.events.is_empty());
    }

    #[test]
    fn interpreter_fails_fast_on_assertion_without_later_side_effects() {
        let mut plan = happy_path_plan();
        plan.body.operations.remove(0);
        for (ordinal, operation) in plan.body.operations.iter_mut().enumerate() {
            match operation {
                BoundSrplOperationPlan::Assert { ordinal: op, .. }
                | BoundSrplOperationPlan::UpdateTable { ordinal: op, .. }
                | BoundSrplOperationPlan::Emit { ordinal: op, .. }
                | BoundSrplOperationPlan::ReadTable { ordinal: op, .. }
                | BoundSrplOperationPlan::Raise { ordinal: op, .. } => *op = ordinal as u32,
            }
        }
        let mut adapter = FakeAdapter {
            assert_passes: false,
            ..FakeAdapter::passing()
        };

        let error = SrplIrInterpreter::execute(&plan, &mut adapter).unwrap_err();

        assert!(matches!(error, SrplExecutionFailure::SemanticViolation(_)));
        assert_eq!(adapter.events, vec!["assert:0", "fail:0"]);
    }

    #[test]
    fn interpreter_rejects_invalid_ordinals_before_side_effects() {
        let plan = plan(vec![BoundSrplOperationPlan::Emit {
            ordinal: 1,
            stream: "Reservation".to_string(),
            values: vec![emit_value()],
        }]);
        let mut adapter = FakeAdapter::passing();

        let error = SrplIrInterpreter::execute(&plan, &mut adapter).unwrap_err();

        assert!(matches!(error, SrplExecutionFailure::SemanticViolation(_)));
        assert!(adapter.events.is_empty());
    }
}
