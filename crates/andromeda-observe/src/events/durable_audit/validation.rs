use andromeda_error::AndromedaResult;

use crate::events::{TraceEvent, observe_error};

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

fn validate_admin_operation_decision_binding(
    record: &PendingDurableAuditRecord,
    family: DurableAuditEventFamily,
) -> AndromedaResult<()> {
    let TraceEvent::AdminOperation(trace) = &record.envelope.event else {
        return Err(observe_error(format!(
            "durable {family:?} records require an admin operation trace",
        )));
    };

    if !record.envelope.correlation.has_request_session() {
        return Err(observe_error(format!(
            "durable {family:?} records require request/session correlation",
        )));
    }

    let binding = &record.principal_binding;
    if binding.certificate_fingerprint.is_none()
        || binding.surface.is_none()
        || binding.permission.is_none()
    {
        return Err(observe_error(format!(
            "durable {family:?} records require certificate, surface, and permission evidence",
        )));
    }
    if binding.request_id != record.envelope.correlation.request_id
        || binding.session_id != record.envelope.correlation.session_id
    {
        return Err(observe_error(format!(
            "durable {family:?} principal binding request/session ids must match envelope correlation",
        )));
    }

    if binding.principal_id != trace.principal.principal_id {
        return Err(observe_error(format!(
            "durable {family:?} principal_id must match admin operation trace",
        )));
    }
    if binding.certificate_fingerprint.as_deref() != Some(trace.certificate.fingerprint.as_str()) {
        return Err(observe_error(format!(
            "durable {family:?} certificate fingerprint must match admin operation trace",
        )));
    }
    if binding.surface != Some(trace.surface) {
        return Err(observe_error(format!(
            "durable {family:?} surface must match admin operation trace",
        )));
    }
    if binding.permission != Some(trace.permission) {
        return Err(observe_error(format!(
            "durable {family:?} permission must match admin operation trace",
        )));
    }

    Ok(())
}
