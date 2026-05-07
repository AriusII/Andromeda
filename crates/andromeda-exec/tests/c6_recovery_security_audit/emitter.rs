use crate::support::{emit_trace, recovery_correlation, wal_replay};
use andromeda_observe::{EventEmitter, InMemoryEventSink, TraceId};

/// Verifies that the `EventEmitter` never drops valid events silently and that
/// acceptance/rejection counters remain observable.
#[test]
fn test_event_emitter_enforces_no_silent_drop_invariant() {
    let mut sink = InMemoryEventSink::new();
    let mut emitter = EventEmitter::new(&mut sink);
    let trace_id = TraceId::new(500);

    for i in 1..=3 {
        let durable_lsn = (i as u64) * 100;
        emit_trace(
            &mut emitter,
            i as u128,
            recovery_correlation(durable_lsn),
            wal_replay(trace_id, durable_lsn),
            "should emit without rejection",
        );
    }

    assert_eq!(
        emitter.accepted_count(),
        3,
        "emitter should have accepted exactly 3 events"
    );
    assert_eq!(
        emitter.rejected_count(),
        0,
        "emitter should have no rejections for valid events"
    );

    let events = emitter.sink().events();
    assert_eq!(events.len(), 3, "sink should contain exactly 3 events");

    for (i, event) in events.iter().enumerate() {
        assert!(
            !event.event_id.is_zero(),
            "event {} should have non-zero EventId",
            i
        );
    }
}
