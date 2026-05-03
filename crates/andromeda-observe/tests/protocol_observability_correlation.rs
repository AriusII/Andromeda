use andromeda_core::{RequestId, SessionId};
use andromeda_observe::{
    BackpressureTrace, CompletionEmittedTrace, ContractRejectedTrace, EventCorrelation,
    EventEnvelope, EventId, EventSink, FrameRejectionTrace, InMemoryEventSink, ProtocolCorrelation,
    ProtocolEventScope, SchemaLayoutDecisionTrace, StreamRoleRejectionTrace, TraceEvent, TraceId,
    UnsupportedVersionTrace,
};

fn request_correlation() -> EventCorrelation {
    EventCorrelation {
        request_id: Some(RequestId::new(10)),
        session_id: Some(SessionId::new(20)),
        contract_hash: None,
        catalog_version: None,
        catalog_object_id: None,
        protocol: Some(protocol_correlation()),
    }
}

fn protocol_correlation() -> ProtocolCorrelation {
    ProtocolCorrelation {
        protocol_version: Some(1),
        stream_id: Some(30),
        stream_role: Some(2),
        frame_type: Some(40),
        payload_kind: Some(50),
        sequence: Some(60),
    }
}

fn valid_backpressure(event_id: u128, trace_id: u128) -> EventEnvelope {
    EventEnvelope::new(
        EventId::new(event_id),
        EventCorrelation::empty(),
        TraceEvent::Backpressure(BackpressureTrace {
            trace_id: TraceId::new(trace_id),
            scope: ProtocolEventScope::Connection,
            protocol: protocol_correlation(),
            retry_after_micros: Some(100),
            pending_units: None,
            limit_units: None,
            reason: "connection-level bounded protocol queue is full".to_string(),
        }),
    )
    .expect("valid connection-scoped backpressure event")
}

#[test]
fn protocol_events_validate_with_neutral_numeric_correlation() {
    let events = [
        EventEnvelope::new(
            EventId::new(1),
            request_correlation(),
            TraceEvent::FrameRejection(FrameRejectionTrace {
                trace_id: TraceId::new(101),
                scope: ProtocolEventScope::Request,
                protocol: protocol_correlation(),
                reason: "frame length exceeds negotiated request limit".to_string(),
            }),
        ),
        EventEnvelope::new(
            EventId::new(2),
            request_correlation(),
            TraceEvent::StreamRoleRejection(StreamRoleRejectionTrace {
                trace_id: TraceId::new(102),
                scope: ProtocolEventScope::Request,
                stream_id: Some(30),
                observed_role: Some(3),
                expected_role: Some(2),
                reason: "stream role does not match request frame".to_string(),
            }),
        ),
        EventEnvelope::new(
            EventId::new(3),
            EventCorrelation::empty(),
            TraceEvent::Backpressure(BackpressureTrace {
                trace_id: TraceId::new(103),
                scope: ProtocolEventScope::Connection,
                protocol: protocol_correlation(),
                retry_after_micros: None,
                pending_units: Some(128),
                limit_units: Some(64),
                reason: "connection protocol queue exceeded high-water mark".to_string(),
            }),
        ),
        EventEnvelope::new(
            EventId::new(4),
            request_correlation(),
            TraceEvent::CompletionEmitted(CompletionEmittedTrace {
                trace_id: TraceId::new(104),
                protocol: protocol_correlation(),
                completion_code: Some(0),
                committed: true,
                durable_lsn: Some(900),
                reason: "completion emitted after durable commit evidence".to_string(),
            }),
        ),
        EventEnvelope::new(
            EventId::new(5),
            request_correlation(),
            TraceEvent::ContractRejected(ContractRejectedTrace {
                trace_id: TraceId::new(105),
                protocol: protocol_correlation(),
                contract_kind: Some(7),
                rejection_code: Some(8),
                reason: "contract hash did not match request manifest".to_string(),
            }),
        ),
        EventEnvelope::new(
            EventId::new(6),
            EventCorrelation::empty(),
            TraceEvent::UnsupportedVersion(UnsupportedVersionTrace {
                trace_id: TraceId::new(106),
                scope: ProtocolEventScope::Connection,
                offered_version: Some(99),
                min_supported_version: Some(1),
                max_supported_version: Some(2),
                reason: "offered protocol version is outside supported range".to_string(),
            }),
        ),
        EventEnvelope::new(
            EventId::new(7),
            request_correlation(),
            TraceEvent::SchemaLayoutDecision(SchemaLayoutDecisionTrace {
                trace_id: TraceId::new(107),
                scope: ProtocolEventScope::Request,
                schema_id: Some(11),
                schema_version: Some(12),
                layout_id: Some(13),
                layout_version: Some(14),
                accepted: true,
                reason: "schema and layout versions match request contract".to_string(),
            }),
        ),
    ];

    for event in events {
        let event = event.expect("protocol event has complete observability evidence");
        assert!(!event.trace_id.is_zero());
    }
}

#[test]
fn incomplete_protocol_evidence_rejects() {
    let missing_request = EventEnvelope::new(
        EventId::new(1),
        EventCorrelation::empty(),
        TraceEvent::CompletionEmitted(CompletionEmittedTrace {
            trace_id: TraceId::new(201),
            protocol: protocol_correlation(),
            completion_code: Some(0),
            committed: false,
            durable_lsn: None,
            reason: "completion is request scoped".to_string(),
        }),
    )
    .unwrap_err();
    assert!(
        missing_request
            .message()
            .contains("request_id and session_id")
    );

    let missing_durable_lsn = EventEnvelope::new(
        EventId::new(2),
        request_correlation(),
        TraceEvent::CompletionEmitted(CompletionEmittedTrace {
            trace_id: TraceId::new(202),
            protocol: protocol_correlation(),
            completion_code: Some(0),
            committed: true,
            durable_lsn: Some(0),
            reason: "committed completion needs durable evidence".to_string(),
        }),
    )
    .unwrap_err();
    assert!(missing_durable_lsn.message().contains("durable LSN"));

    let missing_frame = EventEnvelope::new(
        EventId::new(3),
        request_correlation(),
        TraceEvent::FrameRejection(FrameRejectionTrace {
            trace_id: TraceId::new(203),
            scope: ProtocolEventScope::Request,
            protocol: ProtocolCorrelation::empty(),
            reason: "frame evidence must not be implicit".to_string(),
        }),
    )
    .unwrap_err();
    assert!(missing_frame.message().contains("stream_id and frame_type"));

    let missing_role = EventEnvelope::new(
        EventId::new(4),
        request_correlation(),
        TraceEvent::StreamRoleRejection(StreamRoleRejectionTrace {
            trace_id: TraceId::new(204),
            scope: ProtocolEventScope::Request,
            stream_id: Some(30),
            observed_role: Some(3),
            expected_role: None,
            reason: "role evidence must include expected role".to_string(),
        }),
    )
    .unwrap_err();
    assert!(missing_role.message().contains("expected_role"));

    let missing_backpressure = EventEnvelope::new(
        EventId::new(5),
        EventCorrelation::empty(),
        TraceEvent::Backpressure(BackpressureTrace {
            trace_id: TraceId::new(205),
            scope: ProtocolEventScope::Connection,
            protocol: protocol_correlation(),
            retry_after_micros: None,
            pending_units: None,
            limit_units: None,
            reason: "pressure evidence must be explicit".to_string(),
        }),
    )
    .unwrap_err();
    assert!(missing_backpressure.message().contains("queue pressure"));

    let missing_contract = EventEnvelope::new(
        EventId::new(6),
        request_correlation(),
        TraceEvent::ContractRejected(ContractRejectedTrace {
            trace_id: TraceId::new(206),
            protocol: protocol_correlation(),
            contract_kind: Some(7),
            rejection_code: None,
            reason: "contract rejection code must be explicit".to_string(),
        }),
    )
    .unwrap_err();
    assert!(missing_contract.message().contains("rejection code"));

    let missing_version = EventEnvelope::new(
        EventId::new(7),
        EventCorrelation::empty(),
        TraceEvent::UnsupportedVersion(UnsupportedVersionTrace {
            trace_id: TraceId::new(207),
            scope: ProtocolEventScope::Connection,
            offered_version: Some(99),
            min_supported_version: Some(1),
            max_supported_version: None,
            reason: "version bounds must be explicit".to_string(),
        }),
    )
    .unwrap_err();
    assert!(missing_version.message().contains("version evidence"));

    let missing_schema_layout = EventEnvelope::new(
        EventId::new(8),
        request_correlation(),
        TraceEvent::SchemaLayoutDecision(SchemaLayoutDecisionTrace {
            trace_id: TraceId::new(208),
            scope: ProtocolEventScope::Request,
            schema_id: Some(11),
            schema_version: Some(12),
            layout_id: None,
            layout_version: Some(14),
            accepted: false,
            reason: "schema/layout decision evidence must be explicit".to_string(),
        }),
    )
    .unwrap_err();
    assert!(
        missing_schema_layout
            .message()
            .contains("schema and layout")
    );
}

#[test]
fn in_memory_event_sink_returns_explicit_errors_instead_of_silent_drops() {
    let mut bounded_sink = InMemoryEventSink::with_capacity_limit(1);
    bounded_sink
        .emit(valid_backpressure(1, 301))
        .expect("first event fits bounded sink");

    let capacity_err = bounded_sink.emit(valid_backpressure(2, 302)).unwrap_err();
    assert!(capacity_err.message().contains("capacity exhausted"));
    assert_eq!(bounded_sink.events().len(), 1);

    let mut validating_sink = InMemoryEventSink::new();
    let invalid_event = EventEnvelope {
        event_id: EventId::new(3),
        trace_id: TraceId::new(303),
        correlation: request_correlation(),
        event: TraceEvent::FrameRejection(FrameRejectionTrace {
            trace_id: TraceId::new(303),
            scope: ProtocolEventScope::Request,
            protocol: ProtocolCorrelation::empty(),
            reason: "forged envelope lacks frame evidence".to_string(),
        }),
    };

    let validation_err = validating_sink.emit(invalid_event).unwrap_err();
    assert!(
        validation_err
            .message()
            .contains("stream_id and frame_type")
    );
    assert!(validating_sink.events().is_empty());
}
