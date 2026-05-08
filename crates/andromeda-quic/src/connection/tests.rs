use super::*;
use andromeda_core::CertificateIdentity;
use andromeda_core::{AndromedaErrorKind, RequestId, SessionId};

use crate::frame::{FRAME_HEADER_CRC_UNCHECKED, FrameHeader};
use crate::{FrameBytes, FrameFamily, FrameType};

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

#[test]
fn new_connection_starts_in_hello_state() {
    let conn = Connection::new(SurfacePlane::Application);
    assert_eq!(conn.state(), LifecycleState::Hello);
    assert_eq!(conn.surface_plane(), SurfacePlane::Application);
    assert!(conn.session_id().is_none());
    assert!(!conn.is_active());
}

#[test]
fn rpc_dispatch_rejected_before_handshake() {
    let mut conn = Connection::new(SurfacePlane::Application);
    let exec = frame(FrameType::RpcExecuteRequest, 7);
    let err = conn.dispatch(&exec, SurfacePlane::Application).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
}

#[test]
fn handshake_progresses_hello_then_auth_then_active() {
    let mut conn = Connection::new(SurfacePlane::Application);
    conn.accept_hello(&frame(FrameType::Hello, 42)).unwrap();
    assert_eq!(conn.state(), LifecycleState::Auth);
    assert_eq!(conn.session_id(), Some(SessionId::new(42)));

    conn.accept_auth(&frame(FrameType::Auth, 42)).unwrap();
    assert_eq!(conn.state(), LifecycleState::Active);
    assert!(conn.is_active());

    let exec = frame(FrameType::RpcExecuteRequest, 42);
    let dispatch = conn.dispatch(&exec, SurfacePlane::Application).unwrap();
    assert_eq!(dispatch.frame_type, FrameType::RpcExecuteRequest);
}

#[test]
fn auth_rejected_when_session_id_mismatches_hello() {
    let mut conn = Connection::new(SurfacePlane::Application);
    conn.accept_hello(&frame(FrameType::Hello, 1)).unwrap();
    let err = conn.accept_auth(&frame(FrameType::Auth, 2)).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
}

#[test]
fn auth_frame_in_hello_state_is_rejected() {
    let mut conn = Connection::new(SurfacePlane::Application);
    let err = conn.accept_auth(&frame(FrameType::Auth, 1)).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
}

#[test]
fn surface_mismatch_is_rejected_with_protocol_error() {
    let mut conn = Connection::new(SurfacePlane::Application);
    conn.accept_hello(&frame(FrameType::Hello, 9)).unwrap();
    conn.accept_auth(&frame(FrameType::Auth, 9)).unwrap();
    let exec = frame(FrameType::RpcExecuteRequest, 9);
    let err = conn
        .dispatch(&exec, SurfacePlane::Administration)
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
}

#[test]
fn monitoring_plane_rejects_rpc_command_family() {
    let mut conn = Connection::new(SurfacePlane::Monitoring);
    conn.accept_hello(&frame(FrameType::Hello, 5)).unwrap();
    conn.accept_auth(&frame(FrameType::Auth, 5)).unwrap();
    let exec = frame(FrameType::RpcExecuteRequest, 5);
    let err = conn.dispatch(&exec, SurfacePlane::Monitoring).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);

    let telemetry = frame(FrameType::TelemetrySoftSignal, 5);
    assert!(conn.dispatch(&telemetry, SurfacePlane::Monitoring).is_ok());
}

#[test]
fn drain_rejects_new_commands_but_allows_result_frames() {
    let mut conn = Connection::new(SurfacePlane::Application);
    conn.accept_hello(&frame(FrameType::Hello, 3)).unwrap();
    conn.accept_auth(&frame(FrameType::Auth, 3)).unwrap();
    conn.begin_drain().unwrap();
    assert_eq!(conn.state(), LifecycleState::Draining);

    let exec = frame(FrameType::RpcExecuteRequest, 3);
    let err = conn.dispatch(&exec, SurfacePlane::Application).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);

    // Result-stream frames still flow during drain.
    let batch = frame(FrameType::RpcBatch, 3);
    assert!(conn.dispatch(&batch, SurfacePlane::Application).is_ok());
}

#[test]
fn close_terminates_session_and_blocks_dispatch() {
    let mut conn = Connection::new(SurfacePlane::Application);
    conn.accept_hello(&frame(FrameType::Hello, 11)).unwrap();
    conn.accept_auth(&frame(FrameType::Auth, 11)).unwrap();
    conn.close();
    assert_eq!(conn.state(), LifecycleState::Closed);

    let exec = frame(FrameType::RpcExecuteRequest, 11);
    let err = conn.dispatch(&exec, SurfacePlane::Application).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);

    // Drain after close is a protocol error.
    assert_eq!(
        conn.begin_drain().unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn drain_before_active_is_protocol_error() {
    let mut conn = Connection::new(SurfacePlane::Application);
    let err = conn.begin_drain().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
}

#[test]
fn surface_plane_permits_family_matrix() {
    assert!(SurfacePlane::Application.permits_family(FrameFamily::RpcCommand));
    assert!(SurfacePlane::Administration.permits_family(FrameFamily::ContractControl));
    assert!(SurfacePlane::HighAvailability.permits_family(FrameFamily::RpcCommand));
    assert!(SurfacePlane::Monitoring.permits_family(FrameFamily::Telemetry));
    assert!(!SurfacePlane::Monitoring.permits_family(FrameFamily::RpcCommand));
    assert!(!SurfacePlane::Monitoring.permits_family(FrameFamily::ContractControl));
    // Session control and diagnostic always pass on every plane.
    assert!(SurfacePlane::Application.permits_family(FrameFamily::SessionControl));
    assert!(SurfacePlane::Monitoring.permits_family(FrameFamily::SessionControl));
    assert!(SurfacePlane::Monitoring.permits_family(FrameFamily::Diagnostic));
}

fn cancel(session: u64, cause: CancellationCause) -> CancellationSignal {
    CancellationSignal {
        request_id: RequestId::new(99),
        session_id: SessionId::new(session),
        cause,
    }
}

#[test]
fn cancellation_before_handshake_is_protocol_error() {
    let conn = Connection::new(SurfacePlane::Application);
    let err = conn
        .route_cancellation(&cancel(1, CancellationCause::ClientRequested))
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
}

#[test]
fn cancellation_in_auth_state_is_protocol_error() {
    let mut conn = Connection::new(SurfacePlane::Application);
    conn.accept_hello(&frame(FrameType::Hello, 1)).unwrap();
    let err = conn
        .route_cancellation(&cancel(1, CancellationCause::ClientRequested))
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
}

#[test]
fn cancellation_in_active_state_is_delivered() {
    let mut conn = Connection::new(SurfacePlane::Application);
    conn.accept_hello(&frame(FrameType::Hello, 5)).unwrap();
    conn.accept_auth(&frame(FrameType::Auth, 5)).unwrap();
    let outcome = conn
        .route_cancellation(&cancel(5, CancellationCause::Timeout))
        .unwrap();
    assert_eq!(outcome, CancellationOutcome::Delivered);
}

#[test]
fn cancellation_session_id_mismatch_is_rejected() {
    let mut conn = Connection::new(SurfacePlane::Application);
    conn.accept_hello(&frame(FrameType::Hello, 5)).unwrap();
    conn.accept_auth(&frame(FrameType::Auth, 5)).unwrap();
    let err = conn
        .route_cancellation(&cancel(6, CancellationCause::ClientRequested))
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
}

#[test]
fn cancellation_during_drain_is_delivered_with_drain_outcome() {
    let mut conn = Connection::new(SurfacePlane::Application);
    conn.accept_hello(&frame(FrameType::Hello, 9)).unwrap();
    conn.accept_auth(&frame(FrameType::Auth, 9)).unwrap();
    conn.begin_drain().unwrap();

    let outcome = conn
        .route_cancellation(&cancel(9, CancellationCause::AdminAbort))
        .unwrap();
    assert_eq!(outcome, CancellationOutcome::DeliveredDuringDrain);
}

#[test]
fn session_closed_cause_during_drain_is_protocol_error() {
    let mut conn = Connection::new(SurfacePlane::Application);
    conn.accept_hello(&frame(FrameType::Hello, 9)).unwrap();
    conn.accept_auth(&frame(FrameType::Auth, 9)).unwrap();
    conn.begin_drain().unwrap();

    let err = conn
        .route_cancellation(&cancel(9, CancellationCause::SessionClosed))
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
}

#[test]
fn cancellation_on_closed_session_is_protocol_error() {
    let mut conn = Connection::new(SurfacePlane::Application);
    conn.accept_hello(&frame(FrameType::Hello, 9)).unwrap();
    conn.accept_auth(&frame(FrameType::Auth, 9)).unwrap();
    conn.close();
    let err = conn
        .route_cancellation(&cancel(9, CancellationCause::ClientRequested))
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
}

#[test]
fn certificate_identity_binding_succeeds_when_scope_matches() {
    use andromeda_core::SurfaceScope;

    let mut conn = Connection::new(SurfacePlane::Application);
    let identity =
        CertificateIdentity::new("a".repeat(64), "test-service", SurfaceScope::Application)
            .unwrap();

    assert!(conn.set_certificate_identity(identity).is_ok());
    assert!(conn.certificate_identity().is_some());
}

#[test]
fn certificate_identity_binding_rejects_scope_mismatch() {
    use andromeda_core::SurfaceScope;

    let mut conn = Connection::new(SurfacePlane::Application);
    let identity =
        CertificateIdentity::new("a".repeat(64), "test-admin", SurfaceScope::Administration)
            .unwrap();

    let err = conn.set_certificate_identity(identity).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    assert!(conn.certificate_identity().is_none());
}

#[test]
fn certificate_identity_binding_is_immutable() {
    use andromeda_core::SurfaceScope;

    let mut conn = Connection::new(SurfacePlane::Application);
    let identity1 =
        CertificateIdentity::new("a".repeat(64), "svc1", SurfaceScope::Application).unwrap();
    let identity2 =
        CertificateIdentity::new("b".repeat(64), "svc2", SurfaceScope::Application).unwrap();

    conn.set_certificate_identity(identity1).unwrap();
    let err = conn.set_certificate_identity(identity2).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);

    // First identity remains.
    assert_eq!(
        conn.certificate_identity().unwrap().fingerprint().as_str(),
        "a".repeat(64)
    );
}

#[test]
fn certificate_identity_persists_across_lifecycle() {
    use andromeda_core::SurfaceScope;

    let mut conn = Connection::new(SurfacePlane::Administration);
    let identity = CertificateIdentity::new(
        "c".repeat(64),
        "admin-operator",
        SurfaceScope::Administration,
    )
    .unwrap();

    conn.set_certificate_identity(identity.clone()).unwrap();
    conn.accept_hello(&frame(FrameType::Hello, 1)).unwrap();
    conn.accept_auth(&frame(FrameType::Auth, 1)).unwrap();

    // Identity is still present and unchanged.
    assert_eq!(conn.certificate_identity().unwrap(), &identity);
    assert_eq!(conn.state(), LifecycleState::Active);
}

#[test]
fn ha_dr_plane_requires_cluster_scope() {
    use andromeda_core::SurfaceScope;

    let mut conn = Connection::new(SurfacePlane::HighAvailability);
    let identity =
        CertificateIdentity::new("d".repeat(64), "cluster-node", SurfaceScope::Cluster).unwrap();

    assert!(conn.set_certificate_identity(identity).is_ok());

    // Wrong scope should be rejected.
    let mut conn2 = Connection::new(SurfacePlane::HighAvailability);
    let wrong_identity =
        CertificateIdentity::new("e".repeat(64), "app-svc", SurfaceScope::Application).unwrap();
    assert!(conn2.set_certificate_identity(wrong_identity).is_err());
}

#[test]
fn monitoring_plane_requires_monitoring_agent_scope() {
    use andromeda_core::SurfaceScope;

    let mut conn = Connection::new(SurfacePlane::Monitoring);
    let identity = CertificateIdentity::new(
        "f".repeat(64),
        "telemetry-agent",
        SurfaceScope::MonitoringAgent,
    )
    .unwrap();

    assert!(conn.set_certificate_identity(identity).is_ok());
}
