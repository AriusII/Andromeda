use andromeda_core::AndromedaResult;

use crate::events::{TraceEvent, observe_error};

use super::{
    DurableAuditEventFamily, DurableAuditFailureKind, DurableAuditPrincipalBinding,
    DurableAuditRecordIdentity, DurableAuditSinkFailure, PendingDurableAuditRecord,
    classify_policy_evidence_requirement, durable_audit_family, error::sink_failure,
};

pub(crate) fn validate_record(record: &PendingDurableAuditRecord) -> AndromedaResult<()> {
    record.identity.validate()?;
    record.principal_binding.validate()?;
    record.envelope.validate()?;

    if record.identity.trace_id != record.envelope.trace_id {
        return Err(observe_error(
            "durable audit identity trace_id must match envelope trace_id",
        ));
    }
    if record.identity.event_id != record.envelope.event_id {
        return Err(observe_error(
            "durable audit identity event_id must match envelope event_id",
        ));
    }

    let Some(family) = durable_audit_family(&record.envelope.event) else {
        return Err(observe_error(
            "durable audit record requires an audit-eligible trace event",
        ));
    };
    if family != record.identity.family {
        return Err(observe_error(
            "durable audit identity family must match envelope event family",
        ));
    }

    if matches!(family, DurableAuditEventFamily::SecurityDecision) {
        validate_security_decision_binding(record)?;
    }
    validate_permissioned_critical_policy_binding(
        family,
        &record.principal_binding,
        "durable audit records",
    )?;

    Ok(())
}

pub(crate) fn validate_permissioned_critical_policy_binding(
    family: DurableAuditEventFamily,
    binding: &DurableAuditPrincipalBinding,
    context: &str,
) -> AndromedaResult<()> {
    if !classify_policy_evidence_requirement(family, binding).requires_policy_evidence() {
        return Ok(());
    }

    let Some(policy_version) = binding.policy_version.as_ref() else {
        return Err(observe_error(format!(
            "{context} require policy version evidence for permissioned critical {family:?} records",
        )));
    };
    if !policy_version.has_version_evidence() {
        return Err(observe_error(format!(
            "{context} require non-zero canonical policy version and digest evidence for permissioned critical {family:?} records",
        )));
    }

    Ok(())
}

pub(super) fn validation_failure(
    identity: DurableAuditRecordIdentity,
    message: impl Into<String>,
) -> DurableAuditSinkFailure {
    sink_failure(
        DurableAuditFailureKind::ValidationRejected,
        Some(identity),
        message,
    )
}

fn validate_security_decision_binding(record: &PendingDurableAuditRecord) -> AndromedaResult<()> {
    if !record.envelope.correlation.has_request_session() {
        return Err(observe_error(
            "durable security audit records require request/session correlation",
        ));
    }

    let binding = &record.principal_binding;
    if binding.certificate_fingerprint.is_none()
        || binding.surface.is_none()
        || binding.permission.is_none()
        || binding.policy_version.is_none()
    {
        return Err(observe_error(
            "durable security audit records require certificate, surface, permission, and policy version evidence",
        ));
    }
    if binding.request_id != record.envelope.correlation.request_id
        || binding.session_id != record.envelope.correlation.session_id
    {
        return Err(observe_error(
            "durable security audit principal binding request/session ids must match envelope correlation",
        ));
    }

    if let TraceEvent::SecurityAudit(trace) = &record.envelope.event {
        if binding.principal_id != trace.principal.principal_id {
            return Err(observe_error(
                "durable security audit principal_id must match security audit trace",
            ));
        }
        if binding.certificate_fingerprint.as_deref()
            != Some(trace.certificate.fingerprint.as_str())
        {
            return Err(observe_error(
                "durable security audit certificate fingerprint must match security audit trace",
            ));
        }
        if binding.surface != Some(trace.surface) {
            return Err(observe_error(
                "durable security audit surface must match security audit trace",
            ));
        }
        if binding.permission != Some(trace.permission) {
            return Err(observe_error(
                "durable security audit permission must match security audit trace",
            ));
        }
        if binding.policy_version.as_ref() != Some(&trace.policy_version) {
            return Err(observe_error(
                "durable security audit policy version evidence must match security audit trace",
            ));
        }
    }

    Ok(())
}
