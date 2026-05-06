//! Deterministic SRPL IR interpreter.
//!
//! This module executes the currently bound SRPL operation set:
//! `ReadTable`, `Assert`, `UpdateTable`, `Emit`, and `Raise`.  The interpreter
//! performs a complete validation pass before invoking any adapter method, then
//! executes operations in dense ordinal/source order and fails fast.  There is
//! no storage, transport, transaction, physical execution, ad hoc SQL, or
//! runtime-dispatch dependency in this module; all external behavior is behind
//! the typed adapter traits from [`crate::execution_adapter`].

use andromeda_catalog::{CatalogObjectRef, ObjectKind};
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{
    Cardinality,
    execution_adapter::{
        SrplAssertRequest, SrplAssertResult, SrplAssertionAdapter, SrplBindingEnvironment,
        SrplEmitRequest, SrplEmitResult, SrplExecutionFailure, SrplFailureAdapter,
        SrplFailureRequest, SrplOperationContext, SrplReadRequest, SrplReadResult, SrplRowBound,
        SrplTypedEmitAdapter, SrplTypedReadAdapter, SrplTypedUpdateAdapter, SrplUpdateRequest,
        SrplUpdateResult,
    },
    identifier::validate_srpl_identifier as validate_symbol,
    procedure_model::{
        BoundSrplOperationPlan, ExecutableProcedurePlan, SrplAssignmentIr, SrplEmitValueIr,
        SrplPredicateIr, SrplValueIr,
    },
};

/// Aggregate deterministic execution counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SrplInterpreterReport {
    pub operations_executed: usize,
    pub reads: usize,
    pub assertions: usize,
    pub updates: usize,
    pub emits: usize,
}

/// Empty binding environment used by the deterministic interpreter.
///
/// The current narrow SRPL operation set validates symbol shapes and bounded
/// contracts but does not require dynamic input/binding value lookup at this
/// layer.
struct MinimalBindingEnvironment;

impl SrplBindingEnvironment for MinimalBindingEnvironment {
    fn get_input(&self, _name: &str) -> Option<crate::execution_adapter::SrplBoundValue> {
        None
    }

    fn get_field_from_binding(
        &self,
        _binding: &str,
        _row_index: usize,
        _field: &str,
    ) -> Option<crate::execution_adapter::SrplBoundValue> {
        None
    }

    fn binding_row_count(&self, _binding: &str) -> Option<usize> {
        None
    }
}

/// Stateless interpreter for a catalog-bound executable SRPL plan.
pub struct SrplIrInterpreter;

impl SrplIrInterpreter {
    /// Validates the whole plan before adapter side effects are possible.
    ///
    /// The present bound IR enum has no unsupported variants: every current
    /// variant is matched by this validator and by [`Self::execute`].  Future
    /// variants will require an explicit compiler change because Rust's
    /// exhaustive matching will fail compilation.
    pub fn validate_plan(plan: &ExecutableProcedurePlan) -> AndromedaResult<()> {
        plan.validate()?;

        for operation in &plan.body.operations {
            match operation {
                BoundSrplOperationPlan::ReadTable {
                    source,
                    binding,
                    cardinality,
                    predicates,
                    ..
                } => {
                    source.validate_for_definition(ObjectKind::Table)?;
                    require_evidence_for_table(plan, source)?;
                    validate_symbol(binding, "SRPL read binding")?;
                    validate_bounded_cardinality(*cardinality, "SRPL read")?;
                    validate_predicates(predicates)?;
                }
                BoundSrplOperationPlan::Assert {
                    predicate,
                    failure_code,
                    ..
                } => {
                    validate_predicate(predicate)?;
                    validate_symbol(failure_code, "SRPL assertion failure code")?;
                }
                BoundSrplOperationPlan::UpdateTable {
                    target,
                    predicates,
                    assignments,
                    affected_rows_exact,
                    ..
                } => {
                    target.validate_for_definition(ObjectKind::Table)?;
                    require_evidence_for_table(plan, target)?;
                    validate_predicates(predicates)?;
                    if assignments.is_empty() {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Srpl,
                            "SRPL interpreter update operation must declare assignments",
                        ));
                    }
                    for assignment in assignments {
                        validate_assignment(assignment)?;
                    }
                    match affected_rows_exact {
                        Some(rows) if *rows > 0 => {}
                        Some(_) => {
                            return Err(AndromedaError::new(
                                AndromedaErrorKind::Srpl,
                                "SRPL interpreter update exact affected-row contract must be greater than zero",
                            ));
                        }
                        None => {
                            return Err(AndromedaError::new(
                                AndromedaErrorKind::Srpl,
                                "SRPL interpreter rejects unbounded update affected-row contracts",
                            ));
                        }
                    }
                }
                BoundSrplOperationPlan::Emit { stream, values, .. } => {
                    validate_symbol(stream, "SRPL emit stream")?;
                    if values.is_empty() {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Srpl,
                            "SRPL interpreter emit operation must declare values",
                        ));
                    }
                    for value in values {
                        validate_emit_value(value)?;
                    }
                }
                BoundSrplOperationPlan::Raise { code, .. } => {
                    validate_symbol(code, "SRPL raise code")?;
                }
            }
        }

        Ok(())
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

        let mut report = SrplInterpreterReport::default();
        for operation in &plan.body.operations {
            match operation {
                BoundSrplOperationPlan::ReadTable {
                    ordinal,
                    source,
                    cardinality,
                    predicates,
                    ..
                } => {
                    let bound =
                        row_bound_for_read(*cardinality).map_err(SrplExecutionFailure::from)?;
                    let request = SrplReadRequest::new(
                        context(plan, *ordinal)?,
                        source.clone(),
                        *cardinality,
                        bound,
                        predicates.clone(),
                    )
                    .map_err(SrplExecutionFailure::from)?;
                    let result = adapter.read_typed(request, &MinimalBindingEnvironment)?;
                    SrplReadResult::new(result.rows, *cardinality, bound)?;
                    report.reads += 1;
                }
                BoundSrplOperationPlan::Assert {
                    ordinal,
                    predicate,
                    failure_code,
                } => {
                    let request = SrplAssertRequest::new(
                        context(plan, *ordinal)?,
                        predicate.clone(),
                        failure_code.clone(),
                    )
                    .map_err(SrplExecutionFailure::from)?;
                    let SrplAssertResult { passed } =
                        adapter.assert_typed(request, &MinimalBindingEnvironment)?;
                    report.assertions += 1;
                    if !passed {
                        let failure = SrplExecutionFailure::SemanticViolation(format!(
                            "SRPL assertion failed: {failure_code}"
                        ));
                        adapter.fail_typed(
                            SrplFailureRequest::new(
                                context(plan, *ordinal)?,
                                failure_code.clone(),
                                failure.clone(),
                            )
                            .map_err(SrplExecutionFailure::from)?,
                            &MinimalBindingEnvironment,
                        )?;
                        return Err(failure);
                    }
                }
                BoundSrplOperationPlan::UpdateTable {
                    ordinal,
                    target,
                    predicates,
                    assignments,
                    affected_rows_exact,
                } => {
                    let bound = SrplRowBound::exact(affected_rows_exact.ok_or_else(|| {
                        SrplExecutionFailure::SemanticViolation(
                            "SRPL interpreter rejects unbounded update affected-row contracts"
                                .to_string(),
                        )
                    })?)
                    .map_err(SrplExecutionFailure::from)?;
                    let request = SrplUpdateRequest::new(
                        context(plan, *ordinal)?,
                        target.clone(),
                        bound,
                        predicates.clone(),
                        assignments.clone(),
                    )
                    .map_err(SrplExecutionFailure::from)?;
                    let result = adapter.update_typed(request, &MinimalBindingEnvironment)?;
                    SrplUpdateResult::new(result.affected_rows, bound)?;
                    report.updates += 1;
                }
                BoundSrplOperationPlan::Emit {
                    ordinal,
                    stream,
                    values,
                } => {
                    let bound = SrplRowBound::exact(1).map_err(SrplExecutionFailure::from)?;
                    let request = SrplEmitRequest::new(
                        context(plan, *ordinal)?,
                        stream.clone(),
                        Cardinality::One,
                        bound,
                        values.clone(),
                    )
                    .map_err(SrplExecutionFailure::from)?;
                    let result = adapter.emit_typed(request, &MinimalBindingEnvironment)?;
                    SrplEmitResult::new(result.emitted_rows, Cardinality::One, bound)?;
                    report.emits += 1;
                }
                BoundSrplOperationPlan::Raise { ordinal, code } => {
                    let failure =
                        SrplExecutionFailure::SemanticViolation(format!("SRPL raise: {code}"));
                    adapter.fail_typed(
                        SrplFailureRequest::new(
                            context(plan, *ordinal)?,
                            code.clone(),
                            failure.clone(),
                        )
                        .map_err(SrplExecutionFailure::from)?,
                        &MinimalBindingEnvironment,
                    )?;
                    return Err(failure);
                }
            }
            report.operations_executed += 1;
        }

        Ok(report)
    }
}

fn context(
    plan: &ExecutableProcedurePlan,
    ordinal: u32,
) -> Result<SrplOperationContext, SrplExecutionFailure> {
    SrplOperationContext::new(plan.evidence.procedure_contract, ordinal)
        .map_err(SrplExecutionFailure::from)
}

fn row_bound_for_read(cardinality: Cardinality) -> AndromedaResult<SrplRowBound> {
    match cardinality {
        Cardinality::One => SrplRowBound::exact(1),
        Cardinality::OptionalOne => SrplRowBound::at_most(1),
        Cardinality::Many | Cardinality::NonEmptyMany => Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            "SRPL interpreter rejects read operations without an intrinsic row bound",
        )),
    }
}

fn validate_bounded_cardinality(cardinality: Cardinality, context: &str) -> AndromedaResult<()> {
    if cardinality.intrinsic_max_row_count().is_some() {
        Ok(())
    } else {
        Err(AndromedaError::new(
            AndromedaErrorKind::Srpl,
            format!("{context} operation must carry a bounded row contract"),
        ))
    }
}

fn require_evidence_for_table(
    plan: &ExecutableProcedurePlan,
    object: &CatalogObjectRef,
) -> AndromedaResult<()> {
    let evidence = plan
        .evidence
        .bound_objects
        .iter()
        .find(|bound| bound.object == *object && bound.kind == ObjectKind::Table)
        .ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Contract,
                "SRPL interpreter table operation lacks matching binding evidence",
            )
        })?;
    evidence.validate(ObjectKind::Table)?;
    if evidence.object.catalog_version != plan.evidence.catalog_version {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "SRPL interpreter table evidence catalog version mismatch",
        ));
    }
    Ok(())
}

fn validate_predicates(predicates: &[SrplPredicateIr]) -> AndromedaResult<()> {
    for predicate in predicates {
        validate_predicate(predicate)?;
    }
    Ok(())
}

fn validate_predicate(predicate: &SrplPredicateIr) -> AndromedaResult<()> {
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
            validate_symbol(input, "SRPL predicate input")?;
            validate_symbol(binding, "SRPL predicate binding")?;
            validate_symbol(field, "SRPL predicate field")?;
        }
    }
    Ok(())
}

fn validate_assignment(assignment: &SrplAssignmentIr) -> AndromedaResult<()> {
    validate_symbol(&assignment.field, "SRPL assignment field")?;
    validate_value(&assignment.value)
}

fn validate_emit_value(value: &SrplEmitValueIr) -> AndromedaResult<()> {
    validate_symbol(&value.column, "SRPL emit column")?;
    validate_value(&value.value)
}

fn validate_value(value: &SrplValueIr) -> AndromedaResult<()> {
    match value {
        SrplValueIr::Input(input) => validate_symbol(input, "SRPL value input"),
        SrplValueIr::Field { binding, field } => {
            validate_symbol(binding, "SRPL value binding")?;
            validate_symbol(field, "SRPL value field")
        }
        SrplValueIr::Bool(_) => Ok(()),
        SrplValueIr::SubtractInput {
            binding,
            field,
            input,
        } => {
            validate_symbol(binding, "SRPL subtract binding")?;
            validate_symbol(field, "SRPL subtract field")?;
            validate_symbol(input, "SRPL subtract input")
        }
        SrplValueIr::Constant(literal) => literal.validate(),
        SrplValueIr::BinaryArith { left, right, .. } => {
            validate_value(left)?;
            validate_value(right)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_catalog::{CatalogObjectRef, ProcedureContractRef, QualifiedName};
    use andromeda_core::{CatalogObjectId, CatalogVersion, ContractHash, ProcedureId};

    use crate::procedure_model::{
        BoundSrplBodyPlan, SrplCatalogBindingEvidence, SrplObjectBindingEvidence,
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
