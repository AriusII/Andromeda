use andromeda_core::AndromedaResult;

use super::{observe_error, EventEnvelope};

pub trait EventSink {
    fn emit(&mut self, event: EventEnvelope) -> AndromedaResult<()>;
}

impl<T: EventSink + ?Sized> EventSink for &mut T {
    fn emit(&mut self, event: EventEnvelope) -> AndromedaResult<()> {
        (**self).emit(event)
    }
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryEventSink {
    events: Vec<EventEnvelope>,
    max_events: Option<usize>,
}

impl InMemoryEventSink {
    pub const fn new() -> Self {
        Self {
            events: Vec::new(),
            max_events: None,
        }
    }

    pub const fn with_capacity_limit(max_events: usize) -> Self {
        Self {
            events: Vec::new(),
            max_events: Some(max_events),
        }
    }

    pub fn events(&self) -> &[EventEnvelope] {
        &self.events
    }

    pub fn into_events(self) -> Vec<EventEnvelope> {
        self.events
    }
}

impl EventSink for InMemoryEventSink {
    fn emit(&mut self, event: EventEnvelope) -> AndromedaResult<()> {
        event.validate()?;

        if self
            .max_events
            .is_some_and(|max_events| self.events.len() >= max_events)
        {
            return Err(observe_error(
                "in-memory event sink capacity exhausted; event was not recorded",
            ));
        }

        self.events.push(event);
        Ok(())
    }
}
