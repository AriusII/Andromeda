//! Public-surface contract for cancellation routing and sendable
//! backpressure semantics, including QUIC DATAGRAM bounds.
//!
//! These tests lock the V0 control-plane behavior:
//!
//! * cancellation is gated by lifecycle state and produces typed
//!   [`AndromedaErrorKind::Protocol`] errors when received pre-auth, on a
//!   draining session with a server-only cause, or on a closed session;
//! * backpressure is sendable only on the diagnostic stream or the
//!   telemetry datagram surface; and
//! * backpressure carried over a QUIC DATAGRAM is rejected when the
//!   negotiated MTU is smaller than the encoded upper bound, or when the
//!   reason is request-scoped without a [`RequestId`].

use andromeda_core::{AndromedaErrorKind, RequestId, SessionId};
use andromeda_quic::{
    BackpressureReason, BackpressureSignal, BackpressureTransport, CancellationCause,
    CancellationOutcome, CancellationSignal, Connection, FRAME_HEADER_CRC_UNCHECKED, FrameBytes,
    FrameHeader, FrameType, StreamRole, SurfacePlane,
};

fn frame(frame_type: FrameType, session: u64) -> FrameBytes {
    let payload = match frame_type {
        FrameType::RpcExecuteRequest | FrameType::RpcBatch => b"x".to_vec(),
        _ => Vec::new(),
    };
    FrameBytes {
        header: FrameHeader {
            frame_type,
            request_id: RequestId::new(1),
            session_id: SessionId::new(session),
            tx_id: None,
            payload_length: payload.len() as u64,
            flags: 0,
            header_crc: FRAME_HEADER_CRC_UNCHECKED,
        },
        payload,
    }
}

fn cancel(session: u64, cause: CancellationCause) -> CancellationSignal {
    CancellationSignal {
        request_id: RequestId::new(1),
        session_id: SessionId::new(session),
        cause,
    }
}

#[test]
fn cancellation_pre_auth_is_protocol_error() {
    let conn = Connection::new(SurfacePlane::Application);
    let err = conn
        .route_cancellation(&cancel(1, CancellationCause::ClientRequested))
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
}

#[test]
fn cancellation_in_active_state_is_delivered() {
    let mut conn = Connection::new(SurfacePlane::Application);
    conn.accept_hello(&frame(FrameType::Hello, 11)).unwrap();
    conn.accept_auth(&frame(FrameType::Auth, 11)).unwrap();
    let outcome = conn
        .route_cancellation(&cancel(11, CancellationCause::ClientRequested))
        .unwrap();
    assert_eq!(outcome, CancellationOutcome::Delivered);
}

#[test]
fn cancellation_during_drain_uses_drain_outcome() {
    let mut conn = Connection::new(SurfacePlane::Administration);
    conn.accept_hello(&frame(FrameType::Hello, 12)).unwrap();
    conn.accept_auth(&frame(FrameType::Auth, 12)).unwrap();
    conn.begin_drain().unwrap();

    let outcome = conn
        .route_cancellation(&cancel(12, CancellationCause::AdminAbort))
        .unwrap();
    assert_eq!(outcome, CancellationOutcome::DeliveredDuringDrain);
}

#[test]
fn cancellation_on_closed_session_is_protocol_error() {
    let mut conn = Connection::new(SurfacePlane::Application);
    conn.accept_hello(&frame(FrameType::Hello, 13)).unwrap();
    conn.accept_auth(&frame(FrameType::Auth, 13)).unwrap();
    conn.close();
    let err = conn
        .route_cancellation(&cancel(13, CancellationCause::ClientRequested))
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
}

#[test]
fn backpressure_is_sendable_on_diagnostic_and_datagram_only() {
    let signal = BackpressureSignal {
        reason: BackpressureReason::WalFlushLag,
        request_id: None,
        retry_after_millis: Some(100),
    };

    assert!(signal.validate_routing(StreamRole::Diagnostic).is_ok());
    assert!(
        signal
            .validate_routing(StreamRole::TelemetryDatagram)
            .is_ok()
    );

    for bad in [
        StreamRole::SessionControl,
        StreamRole::CommandBidirectional,
        StreamRole::ResultUnidirectional,
    ] {
        assert_eq!(
            signal.validate_routing(bad).unwrap_err().kind(),
            AndromedaErrorKind::Protocol,
            "backpressure must not be sendable on {bad:?}"
        );
    }
}

#[test]
fn backpressure_datagram_rejects_undersized_mtu() {
    let signal = BackpressureSignal {
        reason: BackpressureReason::HotStorePressure,
        request_id: None,
        retry_after_millis: Some(50),
    };

    let err = signal
        .validate_for_transport(BackpressureTransport::TelemetryDatagram { mtu_bytes: 4 })
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Resource);

    assert!(
        signal
            .validate_for_transport(BackpressureTransport::TelemetryDatagram { mtu_bytes: 1200 })
            .is_ok()
    );
}

#[test]
fn backpressure_datagram_rejects_unaddressed_request_scoped() {
    let signal = BackpressureSignal {
        reason: BackpressureReason::ExecutionQueueSaturated,
        request_id: None,
        retry_after_millis: Some(50),
    };
    assert!(
        signal
            .validate_for_transport(BackpressureTransport::TelemetryDatagram { mtu_bytes: 1500 })
            .is_err()
    );

    let addressed = BackpressureSignal {
        request_id: Some(RequestId::new(3)),
        ..signal
    };
    assert!(
        addressed
            .validate_for_transport(BackpressureTransport::TelemetryDatagram { mtu_bytes: 1500 })
            .is_ok()
    );
}

#[test]
fn backpressure_diagnostic_stream_only_enforces_retry_policy() {
    let signal = BackpressureSignal {
        reason: BackpressureReason::WalFlushLag,
        request_id: None,
        retry_after_millis: Some(250),
    };
    assert!(
        signal
            .validate_for_transport(BackpressureTransport::DiagnosticStream)
            .is_ok()
    );

    let bad = BackpressureSignal {
        retry_after_millis: Some(BackpressureSignal::MAX_RETRY_AFTER_MILLIS + 1),
        ..signal
    };
    assert_eq!(
        bad.validate_for_transport(BackpressureTransport::DiagnosticStream)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Resource
    );
}
