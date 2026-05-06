use andromeda_core::AndromedaResult;

use crate::TraceId;

use super::{EventCorrelation, EventId, TraceEvent};

mod correlation;
mod secret_safety;
mod validation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventEnvelope {
    pub event_id: EventId,
    pub trace_id: TraceId,
    pub correlation: EventCorrelation,
    pub event: TraceEvent,
}

impl EventEnvelope {
    pub fn new(
        event_id: EventId,
        correlation: EventCorrelation,
        event: TraceEvent,
    ) -> AndromedaResult<Self> {
        let envelope = Self {
            event_id,
            trace_id: event.trace_id(),
            correlation,
            event,
        };
        envelope.validate()?;
        Ok(envelope)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        validation::validate(self)
    }
}
