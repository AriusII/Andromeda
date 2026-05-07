use super::*;
use andromeda_types::{ContractHash, RequestId, SessionId};

#[test]
fn event_envelope_validates_non_zero_ids_and_payload_shape() {
    let envelope = EventEnvelope::new(
        EventId::new(10),
        EventCorrelation {
            request_id: Some(RequestId::new(11)),
            session_id: Some(SessionId::new(12)),
            contract_hash: Some(ContractHash::test_vector(7)),
            catalog_version: Some(CatalogVersion::new(13)),
            catalog_object_id: Some(CatalogObjectId::new(14)),
            transaction_id: None,
            durable_lsn: None,
            protocol: Some(ProtocolCorrelation {
                protocol_version: Some(1),
                stream_id: Some(15),
                stream_role: Some(1),
                frame_type: Some(1),
                payload_kind: Some(2),
                sequence: Some(16),
            }),
        },
        TraceEvent::Decision(DecisionTrace {
            trace_id: TraceId::new(9),
            decision: CriticalDecisionKind::ContractValidation,
            reason: "contract hash and catalog version matched request".to_string(),
        }),
    )
    .expect("valid correlated decision envelope");

    assert_eq!(envelope.event_id.get(), 10);
    assert_eq!(envelope.trace_id, TraceId::new(9));

    let err = EventEnvelope::new(
        EventId::new(0),
        EventCorrelation::empty(),
        TraceEvent::Decision(DecisionTrace {
            trace_id: TraceId::new(9),
            decision: CriticalDecisionKind::ContractValidation,
            reason: "valid reason".to_string(),
        }),
    )
    .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Internal);

    let forged = EventEnvelope {
        event_id: EventId::new(1),
        trace_id: TraceId::new(100),
        correlation: EventCorrelation::empty(),
        event: TraceEvent::Decision(DecisionTrace {
            trace_id: TraceId::new(101),
            decision: CriticalDecisionKind::ContractValidation,
            reason: "payload trace diverges from envelope".to_string(),
        }),
    };
    let err = forged.validate().unwrap_err();
    assert!(err.message().contains("must match payload trace_id"));
}

#[test]
fn event_envelope_rejects_empty_decision_reasons() {
    let err = EventEnvelope::new(
        EventId::new(1),
        EventCorrelation::empty(),
        TraceEvent::Decision(DecisionTrace {
            trace_id: TraceId::new(2),
            decision: CriticalDecisionKind::AuthorizationDenial,
            reason: "   ".to_string(),
        }),
    )
    .unwrap_err();

    assert!(err.message().contains("non-empty reason"));
}

#[test]
fn commit_and_recovery_events_require_durable_lsn_evidence() {
    let commit_without_lsn = EventEnvelope::new(
        EventId::new(1),
        EventCorrelation::empty(),
        TraceEvent::CommitVisible(CommitVisibleTrace {
            trace_id: TraceId::new(2),
            transaction_id: TransactionId::new(3),
            durable_commit_lsn: 0,
        }),
    )
    .unwrap_err();
    assert!(commit_without_lsn.message().contains("durable commit LSN"));

    let recovery_without_lsn = EventEnvelope::new(
        EventId::new(4),
        EventCorrelation::empty(),
        TraceEvent::RecoveryStartup(RecoveryTrace {
            trace_id: TraceId::new(5),
            last_durable_lsn: 0,
            corruption_boundary_lsn: None,
        }),
    )
    .unwrap_err();
    assert!(
        recovery_without_lsn
            .message()
            .contains("recovery startup traces")
    );

    assert!(
        EventEnvelope::new(
            EventId::new(6),
            EventCorrelation {
                transaction_id: Some(TransactionId::new(8)),
                durable_lsn: Some(9),
                ..EventCorrelation::empty()
            },
            TraceEvent::CommitVisible(CommitVisibleTrace {
                trace_id: TraceId::new(7),
                transaction_id: TransactionId::new(8),
                durable_commit_lsn: 9,
            }),
        )
        .is_ok()
    );

    assert!(
        EventEnvelope::new(
            EventId::new(10),
            EventCorrelation {
                durable_lsn: Some(12),
                ..EventCorrelation::empty()
            },
            TraceEvent::RecoveryStartup(RecoveryTrace {
                trace_id: TraceId::new(11),
                last_durable_lsn: 12,
                corruption_boundary_lsn: Some(13),
            }),
        )
        .is_ok()
    );
}

#[test]
fn durable_events_require_queryable_transaction_and_lsn_correlation() {
    let append_without_transaction_correlation = EventEnvelope::new(
        EventId::new(20),
        EventCorrelation::empty(),
        TraceEvent::WalEvent(WalEventTrace {
            trace_id: TraceId::new(21),
            transaction_id: Some(TransactionId::new(22)),
            operation: WalOperation::Append,
            appended_lsn: 23,
            durable_lsn: None,
        }),
    )
    .unwrap_err();
    assert!(
        append_without_transaction_correlation
            .message()
            .contains("transaction_id correlation")
    );

    let flush = EventEnvelope::new(
        EventId::new(24),
        EventCorrelation {
            transaction_id: Some(TransactionId::new(22)),
            durable_lsn: Some(25),
            ..EventCorrelation::empty()
        },
        TraceEvent::WalEvent(WalEventTrace {
            trace_id: TraceId::new(26),
            transaction_id: Some(TransactionId::new(22)),
            operation: WalOperation::Flush,
            appended_lsn: 25,
            durable_lsn: Some(25),
        }),
    )
    .expect("WAL flush has matching transaction and durable LSN evidence");
    assert!(flush.correlation.has_transaction_evidence());
    assert!(flush.correlation.has_durable_lsn());

    let commit_visible = EventEnvelope::new(
        EventId::new(27),
        EventCorrelation {
            transaction_id: Some(TransactionId::new(22)),
            durable_lsn: Some(25),
            ..EventCorrelation::empty()
        },
        TraceEvent::CommitVisible(CommitVisibleTrace {
            trace_id: TraceId::new(28),
            transaction_id: TransactionId::new(22),
            durable_commit_lsn: 25,
        }),
    )
    .expect("visible commit points at the durable commit LSN");
    assert_eq!(commit_visible.correlation.durable_lsn, Some(25));
}

#[test]
fn manifest_validation_evidence_is_catalog_correlated_and_secret_safe() {
    let manifest = EventEnvelope::new(
        EventId::new(30),
        EventCorrelation {
            catalog_version: Some(CatalogVersion::new(31)),
            ..EventCorrelation::empty()
        },
        TraceEvent::Manifest(ManifestTrace {
            trace_id: TraceId::new(32),
            event: ManifestEventKind::Validation,
            catalog_version: CatalogVersion::new(31),
            manifest_epoch: 33,
            base_checkpoint_lsn: 34,
            required_wal_start_lsn: 35,
            accepted: true,
            reason: "manifest identity and WAL recovery floor accepted".to_string(),
        }),
    )
    .expect("manifest validation evidence has catalog and WAL anchors");
    assert_eq!(
        manifest.correlation.catalog_version,
        Some(CatalogVersion::new(31))
    );

    let leak = EventEnvelope::new(
        EventId::new(36),
        EventCorrelation {
            catalog_version: Some(CatalogVersion::new(31)),
            ..EventCorrelation::empty()
        },
        TraceEvent::Manifest(ManifestTrace {
            trace_id: TraceId::new(37),
            event: ManifestEventKind::Validation,
            catalog_version: CatalogVersion::new(31),
            manifest_epoch: 33,
            base_checkpoint_lsn: 34,
            required_wal_start_lsn: 35,
            accepted: false,
            reason: "payload: raw manifest body".to_string(),
        }),
    )
    .unwrap_err();
    assert!(leak.message().contains("payload bodies"));
}

#[test]
fn event_envelope_rejects_success_shaped_zero_defaults() {
    let err = EventEnvelope::new(
        EventId::new(1),
        EventCorrelation::empty(),
        TraceEvent::FrameRejection(FrameRejectionTrace {
            trace_id: TraceId::new(0),
            scope: ProtocolEventScope::Connection,
            protocol: ProtocolCorrelation::empty(),
            reason: "reserved frame flag set".to_string(),
        }),
    )
    .unwrap_err();

    assert!(err.message().contains("trace_id must be non-zero"));
}

#[test]
fn event_sink_returns_emit_failures_explicitly() {
    struct FailingSink;

    impl EventSink for FailingSink {
        fn emit(&mut self, _event: EventEnvelope) -> AndromedaResult<()> {
            Err(observe_error("sink unavailable"))
        }
    }

    let mut sink = FailingSink;
    let envelope = EventEnvelope::new(
        EventId::new(1),
        EventCorrelation::empty(),
        TraceEvent::Backpressure(BackpressureTrace {
            trace_id: TraceId::new(2),
            scope: ProtocolEventScope::Connection,
            protocol: ProtocolCorrelation::empty(),
            retry_after_micros: Some(100),
            pending_units: None,
            limit_units: None,
            reason: "bounded queue is full".to_string(),
        }),
    )
    .expect("valid backpressure event");

    let err = sink.emit(envelope).unwrap_err();
    assert_eq!(err.message(), "sink unavailable");
}

#[test]
fn protocol_rejection_crc_constructor_carries_pre_auth_evidence() {
    let trace = ProtocolRejectionTrace::header_crc_mismatch(
        TraceId::new(1),
        Some(7),
        Some(ProtocolSurfacePlane::Application),
        true,
        "frame header CRC mismatch on Hello",
    );
    assert_eq!(trace.reason, ProtocolRejectionReason::HeaderCrcMismatch);
    assert!(trace.pre_auth);
    assert_eq!(trace.scope, ProtocolEventScope::Connection);
    assert!(trace.has_minimum_evidence());
    assert!(trace.reason.is_safety_critical());
}

#[test]
fn protocol_rejection_unsupported_version_requires_version_evidence() {
    let trace = ProtocolRejectionTrace::unsupported_version(
        TraceId::new(2),
        Some(11),
        42,
        true,
        "unsupported QUIC frame codec version",
    );
    assert_eq!(trace.protocol_version, Some(42));
    assert_eq!(trace.reason, ProtocolRejectionReason::UnsupportedVersion);
    assert!(trace.has_minimum_evidence());
    let projected = trace.as_decision_trace();
    assert_eq!(projected.decision, CriticalDecisionKind::UnsupportedVersion);
}

#[test]
fn protocol_rejection_surface_mismatch_records_both_planes() {
    let trace = ProtocolRejectionTrace::surface_plane_mismatch(
        TraceId::new(3),
        Some(99),
        Some(123),
        ProtocolSurfacePlane::Application,
        ProtocolSurfacePlane::Administration,
        Some(5),
        false,
    );
    assert_eq!(trace.reason, ProtocolRejectionReason::SurfacePlaneMismatch);
    assert!(trace.detail.contains("plane code 1"));
    assert!(trace.detail.contains("plane code 2"));
    assert!(trace.has_minimum_evidence());
}

#[test]
fn protocol_rejection_family_blocked_records_frame_type() {
    let trace = ProtocolRejectionTrace::frame_family_blocked(
        TraceId::new(4),
        Some(1),
        Some(2),
        ProtocolSurfacePlane::Monitoring,
        5,
        false,
        "RpcExecuteRequest not permitted on Monitoring plane",
    );
    assert_eq!(
        trace.reason,
        ProtocolRejectionReason::FrameFamilyNotPermitted
    );
    assert_eq!(trace.frame_type_code, Some(5));
    assert!(trace.has_minimum_evidence());
}

#[test]
fn protocol_rejection_sequence_violation_carries_position() {
    let trace = ProtocolRejectionTrace::result_stream_sequence_violation(
        TraceId::new(5),
        Some(101),
        Some(202),
        7,
        Some(3),
        "RpcBatch frame received before RpcMetadata",
    );
    assert_eq!(
        trace.reason,
        ProtocolRejectionReason::ResultStreamSequenceViolation
    );
    assert_eq!(trace.sequence_position, Some(3));
    assert_eq!(trace.scope, ProtocolEventScope::Request);
    assert!(trace.has_minimum_evidence());
}

#[test]
fn protocol_rejection_oversized_payload_records_length_and_scope() {
    let pre = ProtocolRejectionTrace::oversized_payload(
        TraceId::new(6),
        None,
        None,
        None,
        16 * 1024 * 1024 + 1,
        true,
        "frame payload length exceeds maximum during Hello",
    );
    assert_eq!(pre.scope, ProtocolEventScope::Connection);
    assert_eq!(pre.payload_length, Some(16 * 1024 * 1024 + 1));
    assert!(pre.has_minimum_evidence());

    let post = ProtocolRejectionTrace::oversized_payload(
        TraceId::new(7),
        Some(1),
        Some(2),
        Some(5),
        16 * 1024 * 1024 + 1,
        false,
        "frame payload length exceeds maximum",
    );
    assert_eq!(post.scope, ProtocolEventScope::Request);
    assert!(post.has_minimum_evidence());
}

#[test]
fn protocol_rejection_pre_auth_command_marks_pre_auth_true() {
    let trace = ProtocolRejectionTrace::pre_auth_command(
        TraceId::new(8),
        Some(1),
        Some(2),
        5,
        Some(ProtocolSurfacePlane::Application),
        "RPC dispatch attempted before session handshake completed",
    );
    assert!(trace.pre_auth);
    assert_eq!(
        trace.reason,
        ProtocolRejectionReason::PreAuthCommandRejected
    );
    assert!(trace.has_minimum_evidence());
    assert!(trace.reason.is_safety_critical());
}

#[test]
fn protocol_rejection_minimum_evidence_rejects_zero_trace_or_empty_detail() {
    let zero_trace = ProtocolRejectionTrace::header_crc_mismatch(
        TraceId::new(0),
        Some(1),
        None,
        true,
        "crc mismatch",
    );
    assert!(!zero_trace.has_minimum_evidence());

    let empty_detail =
        ProtocolRejectionTrace::header_crc_mismatch(TraceId::new(1), Some(1), None, true, "   ");
    assert!(!empty_detail.has_minimum_evidence());
}

mod transition;
