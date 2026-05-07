//! Deterministic SRPL IR interpreter.
//!
//! This module executes the currently bound SRPL operation set:
//! `ReadTable`, `Assert`, `UpdateTable`, `Emit`, and `Raise`. The interpreter
//! performs a complete validation pass before invoking any adapter method, then
//! executes operations in dense ordinal/source order and fails fast. There is no
//! storage, transport, transaction, physical execution, ad hoc SQL, or
//! runtime-dispatch dependency in this module; all external behavior is behind
//! the typed adapter traits from [`crate::execution_adapter`].

mod diagnostics;
mod execution_state;
mod expression;
mod operation;
mod validation;

use andromeda_core::AndromedaResult;

use crate::{
    execution_adapter::{
        SrplAssertionAdapter, SrplExecutionFailure, SrplFailureAdapter, SrplTypedEmitAdapter,
        SrplTypedReadAdapter, SrplTypedUpdateAdapter,
    },
    procedure_model::ExecutableProcedurePlan,
};

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
    use andromeda_catalog::{CatalogObjectRef, ObjectKind, ProcedureContractRef, QualifiedName};
    use andromeda_core::{CatalogObjectId, CatalogVersion, ContractHash, ProcedureId};

    use crate::{
        Cardinality,
        execution_adapter::{
            SrplAssertRequest, SrplAssertResult, SrplBindingEnvironment, SrplEmitRequest,
            SrplEmitResult, SrplFailureRequest, SrplReadRequest, SrplReadResult,
            SrplTypedReadAdapter, SrplUpdateRequest, SrplUpdateResult,
        },
        procedure_model::{
            BoundSrplBodyPlan, BoundSrplOperationPlan, ExecutableProcedurePlan, SrplAssignmentIr,
            SrplCatalogBindingEvidence, SrplEmitValueIr, SrplObjectBindingEvidence,
            SrplPredicateIr, SrplValueIr,
        },
    };

    #[derive(Default)]
    struct FakeAdapter {
        events: Vec<String>,
        assert_passes: bool,
        read_rows: usize,
        affected_rows: u64,
        emitted_rows: u64,
    }

    impl FakeAdapter {
        fn passing() -> Self {
            Self {
                assert_passes: true,
                read_rows: 1,
                affected_rows: 1,
                emitted_rows: 1,
                events: Vec::new(),
            }
        }
    }

    impl SrplTypedReadAdapter for FakeAdapter {
        type Row = ();

        fn read_typed(
            &mut self,
            request: SrplReadRequest,
            _environment: &dyn SrplBindingEnvironment,
        ) -> Result<SrplReadResult<Self::Row>, SrplExecutionFailure> {
            self.events
                .push(format!("read:{}", request.context.ordinal));
            SrplReadResult::new(
                vec![(); self.read_rows],
                request.cardinality,
                request.row_bound,
            )
        }
    }

    impl SrplAssertionAdapter for FakeAdapter {
        fn assert_typed(
            &mut self,
            request: SrplAssertRequest,
            _environment: &dyn SrplBindingEnvironment,
        ) -> Result<SrplAssertResult, SrplExecutionFailure> {
            self.events
                .push(format!("assert:{}", request.context.ordinal));
            Ok(SrplAssertResult::new(self.assert_passes))
        }
    }

    impl SrplTypedUpdateAdapter for FakeAdapter {
        fn update_typed(
            &mut self,
            request: SrplUpdateRequest,
            _environment: &dyn SrplBindingEnvironment,
        ) -> Result<SrplUpdateResult, SrplExecutionFailure> {
            self.events
                .push(format!("update:{}", request.context.ordinal));
            SrplUpdateResult::new(self.affected_rows, request.affected_rows)
        }
    }

    impl SrplTypedEmitAdapter for FakeAdapter {
        fn emit_typed(
            &mut self,
            request: SrplEmitRequest,
            _environment: &dyn SrplBindingEnvironment,
        ) -> Result<SrplEmitResult, SrplExecutionFailure> {
            self.events
                .push(format!("emit:{}", request.context.ordinal));
            SrplEmitResult::new(self.emitted_rows, request.cardinality, request.row_bound)
        }
    }

    impl SrplFailureAdapter for FakeAdapter {
        fn fail_typed(
            &mut self,
            request: SrplFailureRequest,
            _environment: &dyn SrplBindingEnvironment,
        ) -> Result<(), SrplExecutionFailure> {
            self.events
                .push(format!("fail:{}", request.context.ordinal));
            Ok(())
        }
    }

    fn procedure_ref() -> ProcedureContractRef {
        ProcedureContractRef {
            procedure_id: ProcedureId::new(7),
            contract_hash: ContractHash::test_vector(0xA7),
            catalog_version: CatalogVersion::new(3),
        }
    }

    fn procedure_object() -> CatalogObjectRef {
        CatalogObjectRef {
            object_id: CatalogObjectId::new(99),
            name: QualifiedName::parse("Inventory.ReserveStock").unwrap(),
            kind: ObjectKind::Procedure,
            catalog_version: CatalogVersion::new(3),
        }
    }

    fn stock_object() -> CatalogObjectRef {
        CatalogObjectRef {
            object_id: CatalogObjectId::new(100),
            name: QualifiedName::parse("Inventory.ProductStock").unwrap(),
            kind: ObjectKind::Table,
            catalog_version: CatalogVersion::new(3),
        }
    }

    fn plan(operations: Vec<BoundSrplOperationPlan>) -> ExecutableProcedurePlan {
        ExecutableProcedurePlan {
            procedure_name: QualifiedName::parse("Inventory.ReserveStock").unwrap(),
            body: BoundSrplBodyPlan { operations },
            evidence: SrplCatalogBindingEvidence {
                catalog_version: CatalogVersion::new(3),
                procedure_object: procedure_object(),
                procedure_contract: procedure_ref(),
                bound_objects: vec![SrplObjectBindingEvidence {
                    object: stock_object(),
                    shape_hash: ContractHash::test_vector(0xC1),
                    kind: ObjectKind::Table,
                }],
            },
        }
    }

    fn predicate() -> SrplPredicateIr {
        SrplPredicateIr::InputEqualsField {
            input: "ProductId".to_string(),
            binding: "Stock".to_string(),
            field: "ProductId".to_string(),
        }
    }

    fn assignment() -> SrplAssignmentIr {
        SrplAssignmentIr {
            field: "AvailableQuantity".to_string(),
            value: SrplValueIr::Bool(true),
        }
    }

    fn emit_value() -> SrplEmitValueIr {
        SrplEmitValueIr {
            column: "Reserved".to_string(),
            value: SrplValueIr::Bool(true),
        }
    }

    fn happy_path_plan() -> ExecutableProcedurePlan {
        plan(vec![
            BoundSrplOperationPlan::ReadTable {
                ordinal: 0,
                source: stock_object(),
                binding: "Stock".to_string(),
                cardinality: Cardinality::One,
                predicates: vec![predicate()],
            },
            BoundSrplOperationPlan::Assert {
                ordinal: 1,
                predicate: SrplPredicateIr::FieldGreaterThanOrEqualInput {
                    binding: "Stock".to_string(),
                    field: "AvailableQuantity".to_string(),
                    input: "Quantity".to_string(),
                },
                failure_code: "InsufficientStock".to_string(),
            },
            BoundSrplOperationPlan::UpdateTable {
                ordinal: 2,
                target: stock_object(),
                predicates: vec![predicate()],
                assignments: vec![assignment()],
                affected_rows_exact: Some(1),
            },
            BoundSrplOperationPlan::Emit {
                ordinal: 3,
                stream: "Reservation".to_string(),
                values: vec![emit_value()],
            },
        ])
    }

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
