use super::events_matching;
use crate::events::{InMemoryEventSink, TraceEvent};

impl InMemoryEventSink {
    /// All recorded envelopes whose payload is a `TransactionTransition`
    /// trace. Useful for forensic timelines that need every observed phase
    /// crossing for a transaction without filtering on `kind()` separately.
    pub fn transaction_transition_events(&self) -> Vec<&crate::EventEnvelope> {
        events_matching(self, |envelope| {
            matches!(envelope.event, TraceEvent::TransactionTransition(_))
        })
    }

    /// All recorded envelopes whose payload is an `ExecutionTransition`
    /// trace. Mirrors [`Self::transaction_transition_events`] for the
    /// invocation-side projection.
    pub fn execution_transition_events(&self) -> Vec<&crate::EventEnvelope> {
        events_matching(self, |envelope| {
            matches!(envelope.event, TraceEvent::ExecutionTransition(_))
        })
    }
}
