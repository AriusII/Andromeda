use andromeda_error::AndromedaResult;

use crate::events::{
    DurableAuditPrincipalBinding, Permission, SecurityPolicyVersionEvidence, SurfaceScope,
    TraceEvent, observe_error,
};

use super::{
    DurableAuditAppendRecord, DurableAuditEventFamily, PendingDurableAuditRecord,
    durable_audit_family,
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

    validate_append_projection(record)?;
    if matches!(family, DurableAuditEventFamily::SecurityDecision) {
        validate_security_decision_binding(record)?;
    }
    if matches!(
        family,
        DurableAuditEventFamily::AdminDecision
            | DurableAuditEventFamily::HadrDecision
            | DurableAuditEventFamily::BackupDecision
            | DurableAuditEventFamily::RestoreDecision
            | DurableAuditEventFamily::ForensicDecision
    ) {
        validate_admin_operation_decision_binding(record, family)?;
    }

    Ok(())
}

fn validate_append_projection(record: &PendingDurableAuditRecord) -> AndromedaResult<()> {
    DurableAuditAppendRecord {
        identity: record.identity,
        principal_binding: record.principal_binding.clone(),
        retention: record.retention,
        replay_behavior: record.replay_behavior,
        event_kind: format!("{:?}", record.envelope.event.kind()),
    }
    .validate()
}

fn validate_request_session_correlation(
    record: &PendingDurableAuditRecord,
    subject: &str,
) -> AndromedaResult<()> {
    if !record.envelope.correlation.has_request_session() {
        return Err(observe_error(format!(
            "durable {subject} records require request/session correlation",
        )));
    }
    Ok(())
}

fn validate_required_binding_evidence(
    binding: &DurableAuditPrincipalBinding,
    subject: &str,
    require_policy_version: bool,
) -> AndromedaResult<()> {
    if binding.certificate_fingerprint.is_none()
        || binding.surface.is_none()
        || binding.permission.is_none()
        || (require_policy_version && binding.policy_version.is_none())
    {
        let required_evidence = if require_policy_version {
            "certificate, surface, permission, and policy version evidence"
        } else {
            "certificate, surface, and permission evidence"
        };
        return Err(observe_error(format!(
            "durable {subject} records require {required_evidence}",
        )));
    }
    Ok(())
}

fn validate_binding_correlation(
    record: &PendingDurableAuditRecord,
    subject: &str,
) -> AndromedaResult<()> {
    let binding = &record.principal_binding;
    if binding.request_id != record.envelope.correlation.request_id
        || binding.session_id != record.envelope.correlation.session_id
    {
        return Err(observe_error(format!(
            "durable {subject} principal binding request/session ids must match envelope correlation",
        )));
    }
    Ok(())
}

struct ExpectedAuditBinding<'a> {
    principal_id: &'a str,
    certificate_fingerprint: &'a str,
    surface: SurfaceScope,
    permission: Permission,
    policy_version: Option<&'a SecurityPolicyVersionEvidence>,
}

fn validate_trace_binding(
    binding: &DurableAuditPrincipalBinding,
    subject: &str,
    trace_label: &str,
    expected: ExpectedAuditBinding<'_>,
) -> AndromedaResult<()> {
    if binding.principal_id != expected.principal_id {
        return Err(observe_error(format!(
            "durable {subject} principal_id must match {trace_label} trace",
        )));
    }
    if binding.certificate_fingerprint.as_deref() != Some(expected.certificate_fingerprint) {
        return Err(observe_error(format!(
            "durable {subject} certificate fingerprint must match {trace_label} trace",
        )));
    }
    if binding.surface != Some(expected.surface) {
        return Err(observe_error(format!(
            "durable {subject} surface must match {trace_label} trace",
        )));
    }
    if binding.permission != Some(expected.permission) {
        return Err(observe_error(format!(
            "durable {subject} permission must match {trace_label} trace",
        )));
    }
    if let Some(policy_version) = expected.policy_version
        && binding.policy_version.as_ref() != Some(policy_version)
    {
        return Err(observe_error(format!(
            "durable {subject} policy version evidence must match {trace_label} trace",
        )));
    }
    Ok(())
}

fn validate_security_decision_binding(record: &PendingDurableAuditRecord) -> AndromedaResult<()> {
    const SUBJECT: &str = "security audit";
    validate_request_session_correlation(record, SUBJECT)?;

    let binding = &record.principal_binding;
    validate_required_binding_evidence(binding, SUBJECT, true)?;
    validate_binding_correlation(record, SUBJECT)?;

    if let TraceEvent::SecurityAudit(trace) = &record.envelope.event {
        validate_trace_binding(
            binding,
            SUBJECT,
            "security audit",
            ExpectedAuditBinding {
                principal_id: &trace.principal.principal_id,
                certificate_fingerprint: &trace.certificate.fingerprint,
                surface: trace.surface,
                permission: trace.permission,
                policy_version: Some(&trace.policy_version),
            },
        )?;
    }

    Ok(())
}

fn validate_admin_operation_decision_binding(
    record: &PendingDurableAuditRecord,
    family: DurableAuditEventFamily,
) -> AndromedaResult<()> {
    let TraceEvent::AdminOperation(trace) = &record.envelope.event else {
        return Err(observe_error(format!(
            "durable {family:?} records require an admin operation trace",
        )));
    };
    let subject = format!("{family:?}");

    validate_request_session_correlation(record, &subject)?;

    let binding = &record.principal_binding;
    validate_required_binding_evidence(binding, &subject, true)?;
    validate_binding_correlation(record, &subject)?;

    validate_trace_binding(
        binding,
        &subject,
        "admin operation",
        ExpectedAuditBinding {
            principal_id: &trace.principal.principal_id,
            certificate_fingerprint: &trace.certificate.fingerprint,
            surface: trace.surface,
            permission: trace.permission,
            policy_version: None,
        },
    )?;

    Ok(())
}
