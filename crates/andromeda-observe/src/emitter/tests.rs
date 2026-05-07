use andromeda_error::AndromedaResult;
use andromeda_types::{ContractHash, RequestId, SessionId, TransactionId};

use super::*;
use crate::{
    TraceId,
    events::{
        BackpressureTrace, CompletionEmittedTrace, EventCorrelation, EventEnvelope, EventId,
        EventSink, InMemoryEventSink, ProtocolCorrelation, ProtocolEventScope, TraceEvent,
        observe_error,
    },
};

fn backpressure_event(trace: u128) -> TraceEvent {
    TraceEvent::Backpressure(BackpressureTrace {
        trace_id: TraceId::new(trace),
        scope: ProtocolEventScope::Connection,
        protocol: ProtocolCorrelation::empty(),
        retry_after_micros: Some(100),
        pending_units: None,
        limit_units: None,
        reason: "bounded queue is full".to_string(),
    })
}

fn completion_event(trace: u128) -> TraceEvent {
    TraceEvent::CompletionEmitted(CompletionEmittedTrace {
        trace_id: TraceId::new(trace),
        protocol: ProtocolCorrelation::empty(),
        completion_code: Some(1),
        committed: false,
        durable_lsn: None,
        reason: "request rejected by precondition".to_string(),
    })
}

#[test]
fn emitter_assigns_monotonic_event_ids_on_success() {
    let mut emitter = EventEmitter::new(InMemoryEventSink::new());

    let id_1 = emitter
        .emit(EventCorrelation::empty(), backpressure_event(11))
        .unwrap();
    let id_2 = emitter
        .emit(EventCorrelation::empty(), backpressure_event(12))
        .unwrap();
    let id_3 = emitter
        .emit(EventCorrelation::empty(), backpressure_event(13))
        .unwrap();

    assert_eq!(id_1.get(), 1);
    assert_eq!(id_2.get(), 2);
    assert_eq!(id_3.get(), 3);
    assert_eq!(emitter.accepted_count(), 3);
    assert_eq!(emitter.rejected_count(), 0);
    assert_eq!(emitter.last_event_id(), Some(id_3));
    assert_eq!(emitter.sink().len(), 3);
}

#[test]
fn emitter_surfaces_validation_errors_and_does_not_burn_event_id() {
    let mut emitter = EventEmitter::new(InMemoryEventSink::new());

    // Invalid event: zero trace id is rejected by EventEnvelope::validate.
    let err = emitter
        .emit(EventCorrelation::empty(), backpressure_event(0))
        .unwrap_err();
    assert!(err.message().contains("trace_id must be non-zero"));
    assert_eq!(emitter.rejected_count(), 1);
    assert_eq!(emitter.accepted_count(), 0);
    assert_eq!(emitter.sink().len(), 0);

    // Next valid emit must reuse event_id 1 (no silent gap).
    let id = emitter
        .emit(EventCorrelation::empty(), backpressure_event(7))
        .unwrap();
    assert_eq!(id.get(), 1);
    assert_eq!(emitter.accepted_count(), 1);
    assert_eq!(emitter.rejected_count(), 1);
}

#[test]
fn emitter_propagates_sink_failures_without_silent_success() {
    struct AlwaysFailingSink;
    impl EventSink for AlwaysFailingSink {
        fn emit(&mut self, _event: EventEnvelope) -> AndromedaResult<()> {
            Err(observe_error("sink unavailable"))
        }
    }

    let mut emitter = EventEmitter::new(AlwaysFailingSink);
    let err = emitter
        .emit(EventCorrelation::empty(), backpressure_event(9))
        .unwrap_err();
    assert_eq!(err.message(), "sink unavailable");
    assert_eq!(emitter.accepted_count(), 0);
    assert_eq!(emitter.rejected_count(), 1);
    assert_eq!(emitter.last_event_id(), None);
    // Failed sink writes also reuse the slot so the next attempt re-tries
    // event_id 1 rather than fabricating a phantom gap.
    assert_eq!(emitter.peek_next_event_id().map(EventId::get), Some(1));
}

#[test]
fn emitter_rejects_zero_starting_event_id() {
    let err = EventEmitter::with_starting_event_id(InMemoryEventSink::new(), 0).unwrap_err();
    assert!(err.message().contains("non-zero"));
}

#[test]
fn emitter_envelope_event_id_must_match_allocator_slot() {
    let mut emitter = EventEmitter::with_starting_event_id(InMemoryEventSink::new(), 100).unwrap();

    let envelope = EventEnvelope::new(
        EventId::new(200),
        EventCorrelation::empty(),
        backpressure_event(5),
    )
    .unwrap();

    let err = emitter.emit_envelope(envelope).unwrap_err();
    assert!(err.message().contains("does not match"));
    assert_eq!(emitter.rejected_count(), 1);
    assert_eq!(emitter.sink().len(), 0);

    // A correctly numbered envelope succeeds and advances the allocator.
    let envelope = EventEnvelope::new(
        EventId::new(100),
        EventCorrelation::empty(),
        backpressure_event(5),
    )
    .unwrap();
    let id = emitter.emit_envelope(envelope).unwrap();
    assert_eq!(id.get(), 100);
    assert_eq!(emitter.accepted_count(), 1);
    assert_eq!(emitter.peek_next_event_id().map(EventId::get), Some(101));
}

#[test]
fn emitter_marks_id_space_as_exhausted_after_max() {
    let mut emitter =
        EventEmitter::with_starting_event_id(InMemoryEventSink::new(), u128::MAX).unwrap();

    let _ = emitter
        .emit(EventCorrelation::empty(), backpressure_event(1))
        .unwrap();

    assert!(emitter.is_exhausted());
    let err = emitter
        .emit(EventCorrelation::empty(), backpressure_event(2))
        .unwrap_err();
    assert!(err.message().contains("exhausted"));
    assert_eq!(emitter.accepted_count(), 1);
    assert_eq!(emitter.rejected_count(), 1);
}

#[test]
fn in_memory_sink_query_helpers_pivot_by_correlation() {
    let mut emitter = EventEmitter::new(InMemoryEventSink::new());

    let request_a = RequestId::new(1001);
    let session_a = SessionId::new(2001);
    let session_b = SessionId::new(2002);
    let tx_a = TransactionId::new(3001);
    let contract = ContractHash::test_vector(7);

    let mut corr_a = EventCorrelation::empty();
    corr_a.request_id = Some(request_a);
    corr_a.session_id = Some(session_a);
    corr_a.contract_hash = Some(contract);

    let mut corr_b = EventCorrelation::empty();
    corr_b.session_id = Some(session_b);
    corr_b.transaction_id = Some(tx_a);

    let id_a = emitter.emit(corr_a, backpressure_event(101)).unwrap();
    let id_b = emitter.emit(corr_b, backpressure_event(102)).unwrap();
    let id_c = emitter.emit(corr_a, completion_event(101)).unwrap();

    let sink = emitter.sink();
    assert_eq!(sink.len(), 3);
    assert!(!sink.is_empty());

    let by_trace = sink.events_for_trace(TraceId::new(101));
    assert_eq!(by_trace.len(), 2);
    assert!(by_trace.iter().any(|e| e.event_id == id_a));
    assert!(by_trace.iter().any(|e| e.event_id == id_c));

    let by_request = sink.events_for_request(request_a);
    assert_eq!(by_request.len(), 2);

    let by_session_a = sink.events_for_session(session_a);
    assert_eq!(by_session_a.len(), 2);
    let by_session_b = sink.events_for_session(session_b);
    assert_eq!(by_session_b.len(), 1);
    assert_eq!(by_session_b[0].event_id, id_b);

    let by_tx = sink.events_for_transaction(tx_a);
    assert_eq!(by_tx.len(), 1);
    assert_eq!(by_tx[0].event_id, id_b);

    let by_contract = sink.events_for_contract(contract);
    assert_eq!(by_contract.len(), 2);

    assert_eq!(sink.find_event(id_a).map(|e| e.event_id), Some(id_a));
    assert!(sink.find_event(EventId::new(9999)).is_none());
}

#[test]
fn into_sink_yields_recorded_envelopes() {
    let mut emitter = EventEmitter::new(InMemoryEventSink::new());
    let _ = emitter
        .emit(EventCorrelation::empty(), backpressure_event(1))
        .unwrap();
    let _ = emitter
        .emit(EventCorrelation::empty(), backpressure_event(2))
        .unwrap();

    let sink = emitter.into_sink();
    let events = sink.into_events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_id.get(), 1);
    assert_eq!(events[1].event_id.get(), 2);
}

#[test]
fn emitter_routes_transition_traces_into_sink_with_query_helpers() {
    use crate::events::{
        ExecutionTransitionTrace, TransactionPhaseCode, TransactionTransitionTrace,
        TransitionReasonCode,
    };
    use andromeda_types::InvocationId;

    let mut emitter = EventEmitter::new(InMemoryEventSink::new());

    let request = RequestId::new(11);
    let session = SessionId::new(12);
    let transaction = TransactionId::new(101);

    let mut corr = EventCorrelation::empty();
    corr.request_id = Some(request);
    corr.session_id = Some(session);
    corr.transaction_id = Some(transaction);
    corr.durable_lsn = Some(2024);

    let tx_event = TraceEvent::TransactionTransition(TransactionTransitionTrace {
        trace_id: TraceId::new(701),
        transaction_id: transaction,
        invocation_id: Some(InvocationId::new(40)),
        request_id: Some(request),
        session_id: Some(session),
        prev_phase: TransactionPhaseCode::COMMITTING,
        next_phase: TransactionPhaseCode::COMMITTED,
        durable_lsn: Some(2024),
        reason_code: TransitionReasonCode::DURABLE_WAL_FLUSH,
        reason: "WAL flush proved durable commit".to_string(),
    });
    let exec_event = TraceEvent::ExecutionTransition(ExecutionTransitionTrace {
        trace_id: TraceId::new(702),
        invocation_id: InvocationId::new(40),
        request_id: Some(request),
        session_id: Some(session),
        transaction_id: Some(transaction),
        completion_code: Some(1),
        prev_phase: Some(TransactionPhaseCode::COMMITTING),
        next_phase: Some(TransactionPhaseCode::COMMITTED),
        durable_lsn: Some(2024),
        reason_code: TransitionReasonCode::DURABLE_WAL_FLUSH,
        reason: "executor observed durable commit".to_string(),
    });

    let id_tx = emitter
        .emit(corr, tx_event)
        .expect("tx transition accepted");
    let id_exec = emitter
        .emit(corr, exec_event)
        .expect("exec transition accepted");

    let sink = emitter.sink();
    assert_eq!(sink.transaction_transition_events().len(), 1);
    assert_eq!(sink.execution_transition_events().len(), 1);
    let by_tx = sink.events_for_transaction(transaction);
    assert_eq!(by_tx.len(), 2);
    assert!(by_tx.iter().any(|e| e.event_id == id_tx));
    assert!(by_tx.iter().any(|e| e.event_id == id_exec));
}
