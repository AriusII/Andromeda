use andromeda_srpl_execution_adapter::{SrplExecutionFailure, SrplOperationContext};
use andromeda_srpl_ir::ExecutableProcedurePlan;

pub(super) fn operation_context(
    plan: &ExecutableProcedurePlan,
    ordinal: u32,
) -> Result<SrplOperationContext, SrplExecutionFailure> {
    SrplOperationContext::new(plan.evidence.procedure_contract, ordinal)
        .map_err(SrplExecutionFailure::from)
}

pub(super) fn assertion_failed(failure_code: &str) -> SrplExecutionFailure {
    SrplExecutionFailure::SemanticViolation(format!("SRPL assertion failed: {failure_code}"))
}

pub(super) fn raise_invoked(code: &str) -> SrplExecutionFailure {
    SrplExecutionFailure::SemanticViolation(format!("SRPL raise: {code}"))
}

pub(super) fn unbounded_update_contract() -> SrplExecutionFailure {
    SrplExecutionFailure::SemanticViolation(
        "SRPL interpreter rejects unbounded update affected-row contracts".to_string(),
    )
}
