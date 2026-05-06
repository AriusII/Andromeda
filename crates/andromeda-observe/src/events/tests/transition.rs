use super::*;

fn transition_correlation(
    request_id: u64,
    session_id: u64,
    transaction_id: Option<u64>,
    durable_lsn: Option<u64>,
) -> EventCorrelation {
    EventCorrelation {
        request_id: Some(RequestId::new(request_id)),
        session_id: Some(SessionId::new(session_id)),
        contract_hash: None,
        catalog_version: None,
        catalog_object_id: None,
        transaction_id: transaction_id.map(TransactionId::new),
        durable_lsn,
        protocol: None,
    }
}

#[test]
fn envelope_accepts_terminal_transaction_transition_with_durable_lsn() {
    let trace = TransactionTransitionTrace {
        trace_id: TraceId::new(70),
        transaction_id: TransactionId::new(901),
        invocation_id: Some(InvocationId::new(11)),
        request_id: Some(RequestId::new(3)),
        session_id: Some(SessionId::new(4)),
        prev_phase: TransactionPhaseCode::COMMITTING,
        next_phase: TransactionPhaseCode::COMMITTED,
        durable_lsn: Some(4242),
        reason_code: TransitionReasonCode::DURABLE_WAL_FLUSH,
        reason: "WAL flush proved durable commit boundary".to_string(),
    };

    let envelope = EventEnvelope::new(
        EventId::new(1),
        transition_correlation(3, 4, Some(901), Some(4242)),
        TraceEvent::TransactionTransition(trace),
    )
    .expect("terminal transaction transition envelope must validate");

    assert_eq!(
        envelope.event.kind(),
        CriticalDecisionKind::TransactionTransition
    );
    assert_eq!(envelope.event.trace_id(), TraceId::new(70));
}

#[test]
fn envelope_rejects_terminal_transaction_transition_without_durable_lsn() {
    let trace = TransactionTransitionTrace {
        trace_id: TraceId::new(71),
        transaction_id: TransactionId::new(902),
        invocation_id: None,
        request_id: None,
        session_id: None,
        prev_phase: TransactionPhaseCode::COMMITTING,
        next_phase: TransactionPhaseCode::COMMITTED,
        durable_lsn: None,
        reason_code: TransitionReasonCode::DURABLE_WAL_FLUSH,
        reason: "claims terminal commit without durable LSN".to_string(),
    };

    let err = EventEnvelope::new(
        EventId::new(2),
        transition_correlation(0, 0, Some(902), None),
        TraceEvent::TransactionTransition(trace),
    )
    .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Internal);
}

#[test]
fn envelope_rejects_transaction_transition_correlation_mismatch() {
    let trace = TransactionTransitionTrace {
        trace_id: TraceId::new(72),
        transaction_id: TransactionId::new(903),
        invocation_id: None,
        request_id: Some(RequestId::new(5)),
        session_id: Some(SessionId::new(6)),
        prev_phase: TransactionPhaseCode::ACTIVE,
        next_phase: TransactionPhaseCode::COMMITTING,
        durable_lsn: None,
        reason_code: TransitionReasonCode::NORMAL_PROGRESS,
        reason: "caller requested commit".to_string(),
    };

    // Envelope correlation transaction_id deliberately does not match
    // payload transaction_id.
    let err = EventEnvelope::new(
        EventId::new(3),
        transition_correlation(5, 6, Some(999), None),
        TraceEvent::TransactionTransition(trace),
    )
    .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Internal);
}

#[test]
fn envelope_accepts_execution_transition_committed_with_durable_lsn() {
    let trace = ExecutionTransitionTrace {
        trace_id: TraceId::new(80),
        invocation_id: InvocationId::new(31),
        request_id: Some(RequestId::new(9)),
        session_id: Some(SessionId::new(10)),
        transaction_id: Some(TransactionId::new(555)),
        completion_code: Some(1),
        prev_phase: Some(TransactionPhaseCode::COMMITTING),
        next_phase: Some(TransactionPhaseCode::COMMITTED),
        durable_lsn: Some(7777),
        reason_code: TransitionReasonCode::DURABLE_WAL_FLUSH,
        reason: "execution observed durable commit".to_string(),
    };

    let envelope = EventEnvelope::new(
        EventId::new(4),
        transition_correlation(9, 10, Some(555), Some(7777)),
        TraceEvent::ExecutionTransition(trace),
    )
    .expect("committed execution transition envelope must validate");

    assert_eq!(
        envelope.event.kind(),
        CriticalDecisionKind::ExecutionTransition
    );
}

#[test]
fn envelope_rejects_execution_transition_terminal_without_durable_lsn() {
    let trace = ExecutionTransitionTrace {
        trace_id: TraceId::new(81),
        invocation_id: InvocationId::new(32),
        request_id: Some(RequestId::new(9)),
        session_id: Some(SessionId::new(10)),
        transaction_id: Some(TransactionId::new(556)),
        completion_code: Some(2),
        prev_phase: Some(TransactionPhaseCode::ROLLING_BACK),
        next_phase: Some(TransactionPhaseCode::ROLLED_BACK),
        durable_lsn: None,
        reason_code: TransitionReasonCode::DURABLE_WAL_FLUSH,
        reason: "claims rollback durable without LSN".to_string(),
    };

    let err = EventEnvelope::new(
        EventId::new(5),
        transition_correlation(9, 10, Some(556), None),
        TraceEvent::ExecutionTransition(trace),
    )
    .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Internal);
}

#[test]
fn envelope_rejects_pre_transaction_rejection_carrying_transaction_or_lsn() {
    // Payload-level forging is already blocked by the trace's own
    // validate(); this test exercises the envelope-level guard against
    // smuggling transaction/durable_lsn evidence through the correlation
    // header on a pre-transaction rejection path.
    let trace = ExecutionTransitionTrace {
        trace_id: TraceId::new(82),
        invocation_id: InvocationId::new(33),
        request_id: Some(RequestId::new(9)),
        session_id: Some(SessionId::new(10)),
        transaction_id: None,
        completion_code: Some(8),
        prev_phase: None,
        next_phase: None,
        durable_lsn: None,
        reason_code: TransitionReasonCode::PRE_TRANSACTION_REJECTION,
        reason: "contract validation failed before any transaction".to_string(),
    };

    let envelope_with_tx = EventEnvelope::new(
        EventId::new(6),
        transition_correlation(9, 10, Some(123), None),
        TraceEvent::ExecutionTransition(trace.clone()),
    );
    assert!(envelope_with_tx.is_err());

    let envelope_with_lsn = EventEnvelope::new(
        EventId::new(7),
        transition_correlation(9, 10, None, Some(42)),
        TraceEvent::ExecutionTransition(trace.clone()),
    );
    assert!(envelope_with_lsn.is_err());

    let envelope_clean = EventEnvelope::new(
        EventId::new(8),
        transition_correlation(9, 10, None, None),
        TraceEvent::ExecutionTransition(trace),
    )
    .expect("clean pre-transaction rejection envelope must validate");
    assert_eq!(
        envelope_clean.event.kind(),
        CriticalDecisionKind::ExecutionTransition
    );
}

#[test]
fn in_memory_event_sink_records_and_queries_transition_events() {
    let mut sink = InMemoryEventSink::new();

    let tx_trace = TransactionTransitionTrace {
        trace_id: TraceId::new(90),
        transaction_id: TransactionId::new(700),
        invocation_id: None,
        request_id: Some(RequestId::new(2)),
        session_id: Some(SessionId::new(3)),
        prev_phase: TransactionPhaseCode::ACTIVE,
        next_phase: TransactionPhaseCode::COMMITTING,
        durable_lsn: None,
        reason_code: TransitionReasonCode::NORMAL_PROGRESS,
        reason: "caller requested commit".to_string(),
    };
    let tx_envelope = EventEnvelope::new(
        EventId::new(1),
        transition_correlation(2, 3, Some(700), None),
        TraceEvent::TransactionTransition(tx_trace),
    )
    .expect("tx transition envelope valid");

    let exec_trace = ExecutionTransitionTrace {
        trace_id: TraceId::new(91),
        invocation_id: InvocationId::new(40),
        request_id: Some(RequestId::new(2)),
        session_id: Some(SessionId::new(3)),
        transaction_id: Some(TransactionId::new(700)),
        completion_code: Some(1),
        prev_phase: Some(TransactionPhaseCode::COMMITTING),
        next_phase: Some(TransactionPhaseCode::COMMITTED),
        durable_lsn: Some(8181),
        reason_code: TransitionReasonCode::DURABLE_WAL_FLUSH,
        reason: "exec observed durable commit".to_string(),
    };
    let exec_envelope = EventEnvelope::new(
        EventId::new(2),
        transition_correlation(2, 3, Some(700), Some(8181)),
        TraceEvent::ExecutionTransition(exec_trace),
    )
    .expect("exec transition envelope valid");

    sink.emit(tx_envelope).expect("sink accepts tx transition");
    sink.emit(exec_envelope)
        .expect("sink accepts exec transition");

    assert_eq!(sink.transaction_transition_events().len(), 1);
    assert_eq!(sink.execution_transition_events().len(), 1);
    assert_eq!(
        sink.events_for_transaction(TransactionId::new(700)).len(),
        2
    );
}
