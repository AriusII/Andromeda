use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, PipelineClass};

use crate::{CompletionStatus, ExecutionIoAdmissionDecision, InvocationReject, RollbackCause};

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
                }
                RollbackCause::Poison => "poison rollback reason must not be empty",
            },
        ));
    }

    let domain: &[u8] = match cause {
        // Preserve the historical business-validation domain tag so existing
        // recovery tools and tests keep parsing the WAL payload format.
        RollbackCause::Direct | RollbackCause::BusinessFailure => {
            b"andromeda.exec.business-validation-failed.v1"
        }
        RollbackCause::Poison => b"andromeda.exec.poisoned-rollback.v1",
    };
    let mut payload = Vec::with_capacity(domain.len() + 1 + trimmed_reason.len());
    payload.extend_from_slice(domain);
    payload.push(0);
    payload.extend_from_slice(trimmed_reason.as_bytes());
    Ok(payload)
}
