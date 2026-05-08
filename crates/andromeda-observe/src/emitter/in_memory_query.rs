mod correlation;
mod transition;

use crate::events::{EventEnvelope, InMemoryEventSink};

fn events_matching(
    sink: &InMemoryEventSink,
    mut predicate: impl FnMut(&EventEnvelope) -> bool,
) -> Vec<&EventEnvelope> {
    sink.events()
        .iter()
        .filter(|envelope| predicate(envelope))
        .collect()
}
