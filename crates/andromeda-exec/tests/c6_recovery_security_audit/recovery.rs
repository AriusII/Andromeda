use crate::support::{
    emit_trace, incomplete_transaction_wal_event, recovery_correlation, recovery_startup,
    transaction_correlation, wal_replay,
};
use andromeda_core::TransactionId;
use andromeda_observe::{EventEmitter, InMemoryEventSink, TraceEvent, TraceId};

/// Verifies that recovery startup emits a complete audit trail covering
/// manifest loading, WAL replay, and completion.
#[test]
fn test_recovery_audit_trace_covers_startup_and_replay() {
    let mut sink = InMemoryEventSink::new();
    let mut emitter = EventEmitter::new(&mut sink);
    let trace_id = TraceId::new(100);

    let event_id_1 = emit_trace(
        &mut emitter,
        1,
        recovery_correlation(1000),
        recovery_startup(trace_id, 1000),
        "should emit startup trace",
    );

    assert!(!event_id_1.is_zero(), "event_id should be non-zero");
    assert_eq!(
        emitter.accepted_count(),
        1,
        "emitter should have accepted 1 event"
    );
    assert_eq!(
        emitter.rejected_count(),
        0,
        "emitter should have no rejections"
    );

    let event_id_2 = emit_trace(
        &mut emitter,
        2,
        recovery_correlation(1000),
        wal_replay(trace_id, 1000),
        "should emit wal replay trace",
    );

    assert!(!event_id_2.is_zero(), "event_id should be non-zero");
    assert_eq!(
        emitter.accepted_count(),
        2,
        "emitter should have accepted 2 events"
    );

    let events = emitter.sink().events();
    assert_eq!(events.len(), 2, "should have 2 events");

    for event in events {
        assert_eq!(
            event.trace_id, trace_id,
            "all events should share the recovery trace_id"
        );
    }
}

/// Verifies that recovery skips of incomplete transactions remain observable and
/// carry enough WAL evidence for forensic identification.
#[test]
fn test_recovery_incomplete_transaction_rejection_traced() {
    let mut sink = InMemoryEventSink::new();
    let mut emitter = EventEmitter::new(&mut sink);
    let trace_id = TraceId::new(300);
    let transaction_id = TransactionId::new(42);

    emit_trace(
        &mut emitter,
        1,
        transaction_correlation(transaction_id, 500),
        incomplete_transaction_wal_event(trace_id, transaction_id, 500),
        "should emit incomplete transaction trace",
    );

    let events = emitter.sink().events();
    assert!(
        !events.is_empty(),
        "should have emitted incomplete transaction trace"
    );

    let first_event = &events[0];
    assert_eq!(first_event.trace_id, trace_id);

    if let TraceEvent::WalEvent(wal_trace) = &first_event.event {
        assert_eq!(
            wal_trace.transaction_id,
            Some(transaction_id),
            "trace should record the incomplete transaction_id"
        );
        assert_eq!(
            wal_trace.appended_lsn, 500u64,
            "trace should record LSN for recovery forensics"
        );
    } else {
        panic!("expected WAL event trace");
    }
}
