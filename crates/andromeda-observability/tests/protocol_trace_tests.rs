use andromeda_observability::{
    CriticalDecisionKind, ProtocolEventScope, ProtocolRejectionReason, ProtocolRejectionTrace,
    ProtocolSurfacePlane, SchemaLayoutDecisionTrace, TraceId,
};

#[test]
fn schema_layout_decision_reports_evidence_and_reason() {
    let trace = SchemaLayoutDecisionTrace {
        trace_id: TraceId::new(9),
        scope: ProtocolEventScope::Request,
        schema_id: Some(11),
        schema_version: Some(12),
        layout_id: Some(13),
        layout_version: Some(14),
        accepted: true,
        reason: "schema and layout versions match request contract".to_string(),
    };

    assert!(trace.has_reason());
    assert!(trace.has_schema_layout_evidence());
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
