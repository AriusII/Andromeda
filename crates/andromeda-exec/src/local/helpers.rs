use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_hardware::{PipelineClass, ResourceBudget};
use andromeda_observe::TraceId;
use andromeda_storage_page::PageSize;
use andromeda_storage_placement::{
    CoreIoPlacementRequest, OperationalProfile, StorageIoBudgetScope, StorageWorkloadClass,
};

use crate::{
    CompletionStatus, ExecutionIoAdmissionDecision, ExecutionIoAdmissionRequest, InvocationReject,
    RollbackCause,
};

pub fn require_local_procedure_execution_io_admission(
    io_admission: Result<ExecutionIoAdmissionDecision, InvocationReject>,
) -> AndromedaResult<ExecutionIoAdmissionDecision> {
    let decision = io_admission.map_err(|reject| {
        AndromedaError::new(
            io_admission_error_kind(reject.status),
            format!(
                "execution IO admission rejected before local procedure execution: {}",
                reject.reason
            ),
        )
    })?;

    if decision.pipeline_class != PipelineClass::ForegroundExecution {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            format!(
                "execution IO admission decision for local procedure execution must target {} pipeline, got {}",
                PipelineClass::ForegroundExecution.name(),
                decision.pipeline_class.name()
            ),
        ));
    }

    if !decision.trace.has_explanation() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "execution IO admission decision for local procedure execution must include an explicit reason",
        ));
    }

    Ok(decision)
}

pub(super) fn require_local_procedure_execution_io_admission_for_trace(
    io_admission: Result<ExecutionIoAdmissionDecision, InvocationReject>,
    trace_id: TraceId,
) -> AndromedaResult<ExecutionIoAdmissionDecision> {
    let decision = require_local_procedure_execution_io_admission(io_admission)?;
    if decision.trace.trace_id != trace_id {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "execution IO admission evidence trace id must match local Procedure invocation trace",
        ));
    }

    Ok(decision)
}

pub(super) fn default_local_procedure_execution_io_admission(
    trace_id: TraceId,
) -> AndromedaResult<ExecutionIoAdmissionDecision> {
    let profile = OperationalProfile::hot_write();
    let placement_request = CoreIoPlacementRequest::new(
        StorageWorkloadClass::HotAppend,
        StorageIoBudgetScope::Page(PageSize::KiB16),
        profile.workflow.page_budget.path_budget,
        false,
    );
    let request = ExecutionIoAdmissionRequest::new(
        profile,
        PipelineClass::ForegroundExecution,
        ResourceBudget::new(PageSize::KiB16.bytes() as u64, 0, 1),
        placement_request,
    );

    require_local_procedure_execution_io_admission_for_trace(
        request.validate_admission(trace_id),
        trace_id,
    )
}

pub(super) fn io_admission_error_kind(status: CompletionStatus) -> AndromedaErrorKind {
    match status {
        CompletionStatus::PermissionDenied => AndromedaErrorKind::Security,
        CompletionStatus::ContractRejected => AndromedaErrorKind::Contract,
        CompletionStatus::SystemUnavailable => AndromedaErrorKind::Resource,
        CompletionStatus::Committed
        | CompletionStatus::RolledBack
        | CompletionStatus::FailedBeforeTransaction
        | CompletionStatus::Cancelled
        | CompletionStatus::Poisoned => AndromedaErrorKind::Execution,
    }
}

pub(super) fn rollback_payload_for_cause(
    cause: RollbackCause,
    reason: &str,
) -> AndromedaResult<Vec<u8>> {
    let trimmed_reason = reason.trim();
    if trimmed_reason.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Execution,
            match cause {
                RollbackCause::Direct | RollbackCause::BusinessFailure => {
                    "business validation rollback reason must not be empty"
                },
                RollbackCause::Poison => "poison rollback reason must not be empty",
            },
        ));
    }

    let domain: &[u8] = match cause {
        RollbackCause::Direct | RollbackCause::BusinessFailure => {
            b"andromeda.exec.business-validation-failed.v1"
        },
        RollbackCause::Poison => b"andromeda.exec.poisoned-rollback.v1",
    };
    let mut payload = Vec::with_capacity(domain.len() + 1 + trimmed_reason.len());
    payload.extend_from_slice(domain);
    payload.push(0);
    payload.extend_from_slice(trimmed_reason.as_bytes());
    Ok(payload)
}
