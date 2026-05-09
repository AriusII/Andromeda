use andromeda_error::AndromedaResult;

use crate::events::{EventEnvelope, TraceEvent, observe_error};

pub(super) fn validate_identity(envelope: &EventEnvelope) -> AndromedaResult<()> {
    if envelope.event_id.is_zero() {
        return Err(observe_error("observability event_id must be non-zero"));
    }

    if envelope.trace_id.is_zero() {
        return Err(observe_error("observability trace_id must be non-zero"));
    }

    if envelope.trace_id != envelope.event.trace_id() {
        return Err(observe_error(
            "observability envelope trace_id must match payload trace_id",
        ));
    }

    Ok(())
}

pub(super) fn validate_transition_payloads(envelope: &EventEnvelope) -> AndromedaResult<()> {
    match &envelope.event {
        TraceEvent::TransactionTransition(trace) => trace.validate(),
        TraceEvent::ExecutionTransition(trace) => trace.validate(),
        _ => Ok(()),
    }
}
