use crate::TraceEvent;

pub(super) fn principal_of(event: &TraceEvent) -> Option<&str> {
    match event {
        TraceEvent::SecurityAudit(trace) => Some(trace.principal.principal_id.as_str()),
        TraceEvent::AdminOperation(trace) => Some(trace.principal.principal_id.as_str()),
        TraceEvent::Audit(trace) => Some(trace.actor.as_str()),
        _ => None,
    }
}
