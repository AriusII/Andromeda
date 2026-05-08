use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observe::{DurableAuditSinkReport, TraceId};

pub use andromeda_execution_trace::{
    CompletionAuditEvidence, CompletionAuditPolicy, CompletionEmission, InvocationCompletionEmitter,
};

use crate::services::permission_audit_emitter::{
    AuditEmissionEvidence, AuditEmissionKind, AuditEmissionOutcome, AuditEmissionPolicy,
    AuditSinkAvailability,
};

impl CompletionAuditEvidence for AuditEmissionEvidence {
    fn validate_completion_audit(&self, expected_trace_id: TraceId) -> AndromedaResult<()> {
        self.validate()?;
        if self.kind != AuditEmissionKind::Completion {
            return Err(completion_audit_error(
                "completion emission requires completion audit evidence",
            ));
        }
        if self.outcome != AuditEmissionOutcome::Emitted {
            return Err(completion_audit_error(
                "completion audit evidence requires emitted outcome",
            ));
        }
        if self.trace_id != expected_trace_id {
            return Err(completion_audit_error(
                "completion audit evidence trace id must match emitted completion",
            ));
        }
        Ok(())
    }
}

impl CompletionAuditPolicy<AuditSinkAvailability> for AuditEmissionPolicy {
    type Evidence = AuditEmissionEvidence;

    fn completion_emitted(
        &self,
        trace_id: TraceId,
        reason: String,
        sink: AuditSinkAvailability,
    ) -> AndromedaResult<Self::Evidence> {
        AuditEmissionEvidence::completion_emitted(*self, trace_id, reason, sink)
    }

    fn sink_from_durable_report(
        &self,
        report: DurableAuditSinkReport,
    ) -> AndromedaResult<AuditSinkAvailability> {
        AuditSinkAvailability::durable_for_policy(*self, report)
    }
}

fn completion_audit_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Execution, message)
}
