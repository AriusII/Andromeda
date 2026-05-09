use std::time::SystemTime;

pub(crate) use andromeda_audit::{
    AdmissionAuditEvent, AdmissionDecisionKind, AffectedPrincipal, BackpressureReason,
    ContractValidationResult, ProcedureId, TraceId,
};

pub(crate) fn test_trace_id() -> TraceId {
    TraceId::new(12345)
}

pub(crate) fn test_procedure_id() -> ProcedureId {
    ProcedureId::new(5001)
}

pub(crate) fn test_principal(name: &str) -> AffectedPrincipal {
    AffectedPrincipal::new(format!("CN=test-principal-{}", name))
}

pub(crate) fn now() -> SystemTime {
    SystemTime::now()
}

pub(crate) fn contract_validated_event_at(
    principal_name: &str,
    input_row_count: u64,
    result: ContractValidationResult,
    validation_details: impl Into<String>,
    event_timestamp: SystemTime,
) -> AdmissionAuditEvent {
    AdmissionAuditEvent::ContractValidated {
        trace_id: test_trace_id(),
        procedure_id: test_procedure_id(),
        input_row_count,
        result,
        validation_details: validation_details.into(),
        affected_principal: test_principal(principal_name),
        event_timestamp,
    }
}

pub(crate) fn contract_validated_event(
    principal_name: &str,
    input_row_count: u64,
    result: ContractValidationResult,
    validation_details: impl Into<String>,
) -> AdmissionAuditEvent {
    contract_validated_event_at(
        principal_name,
        input_row_count,
        result,
        validation_details,
        now(),
    )
}

pub(crate) fn admission_decision_event(
    principal_name: &str,
    decision: AdmissionDecisionKind,
    reason: impl Into<String>,
) -> AdmissionAuditEvent {
    AdmissionAuditEvent::AdmissionDecision {
        trace_id: test_trace_id(),
        procedure_id: test_procedure_id(),
        decision,
        reason: reason.into(),
        affected_principal: test_principal(principal_name),
        event_timestamp: now(),
    }
}

pub(crate) fn permission_check_failed_event(
    principal_name: &str,
    required_permission: impl Into<String>,
    actual_permission_set: impl Into<String>,
) -> AdmissionAuditEvent {
    AdmissionAuditEvent::PermissionCheckFailed {
        trace_id: test_trace_id(),
        procedure_id: test_procedure_id(),
        affected_principal: test_principal(principal_name),
        required_permission: required_permission.into(),
        actual_permission_set: actual_permission_set.into(),
        event_timestamp: now(),
    }
}

pub(crate) fn request_throttled_event(
    backpressure_reason: BackpressureReason,
    retry_after_ms: u64,
) -> AdmissionAuditEvent {
    AdmissionAuditEvent::RequestThrottled {
        trace_id: test_trace_id(),
        procedure_id: test_procedure_id(),
        backpressure_reason,
        retry_after_ms,
        event_timestamp: now(),
    }
}

pub(crate) fn procedure_dispatch_authorized_event(
    principal_name: &str,
    surface_plane: impl Into<String>,
    certificate_identity: impl Into<String>,
) -> AdmissionAuditEvent {
    AdmissionAuditEvent::ProcedureDispatchAuthorized {
        trace_id: test_trace_id(),
        procedure_id: test_procedure_id(),
        surface_plane: surface_plane.into(),
        certificate_identity: certificate_identity.into(),
        affected_principal: test_principal(principal_name),
        dispatch_timestamp: now(),
    }
}
