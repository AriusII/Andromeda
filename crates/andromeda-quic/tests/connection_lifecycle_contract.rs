//! Public-surface contract for the QUIC connection lifecycle and surface
//! plane gating. These tests exercise `andromeda_quic::Connection` through
//! its exported API only, locking the V0 transport contract.

use andromeda_error::AndromedaErrorKind;
use andromeda_quic::{
    CancellationCause, CancellationOutcome, CancellationSignal, Connection, DatagramPolicy,
    EarlyDataPolicy, LifecycleState, SurfaceListenerConfig, SurfaceListenerSet, SurfacePlane,
};
use andromeda_rpc_protocol::{FRAME_HEADER_CRC_UNCHECKED, FrameBytes, FrameHeader, FrameType};
use andromeda_types::{RequestId, SessionId};

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
fn pre_auth_rpc_dispatch_is_protocol_error() {
    let mut conn = Connection::new(SurfacePlane::Application);
    assert_eq!(conn.state(), LifecycleState::Hello);
    let err = conn
        .dispatch(
            &frame(FrameType::RpcExecuteRequest, 1),
            SurfacePlane::Application,
        )
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
}

#[test]
fn full_handshake_then_active_dispatch() {
    let mut conn = Connection::new(SurfacePlane::Application);
    conn.accept_hello(&frame(FrameType::Hello, 77)).unwrap();
    conn.accept_auth(&frame(FrameType::Auth, 77)).unwrap();
    assert_eq!(conn.state(), LifecycleState::Active);
    assert_eq!(conn.session_id(), Some(SessionId::new(77)));
    let dispatched = conn
        .dispatch(
            &frame(FrameType::RpcExecuteRequest, 77),
            SurfacePlane::Application,
        )
        .unwrap();
    assert_eq!(dispatched.frame_type, FrameType::RpcExecuteRequest);
}

#[test]
fn active_dispatch_rejects_frame_from_different_session() {
    let mut conn = Connection::new(SurfacePlane::Application);
    conn.accept_hello(&frame(FrameType::Hello, 77)).unwrap();
    conn.accept_auth(&frame(FrameType::Auth, 77)).unwrap();

    let err = conn
        .dispatch(
            &frame(FrameType::RpcExecuteRequest, 78),
            SurfacePlane::Application,
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    assert!(
        err.message().contains("session id"),
        "session binding rejection should name session id"
    );
}

#[test]
fn drain_then_close_lifecycle() {
    let mut conn = Connection::new(SurfacePlane::Administration);
    conn.accept_hello(&frame(FrameType::Hello, 8)).unwrap();
    conn.accept_auth(&frame(FrameType::Auth, 8)).unwrap();
    conn.begin_drain().unwrap();
    assert_eq!(conn.state(), LifecycleState::Draining);

    // Idempotent drain.
    conn.begin_drain().unwrap();

    conn.close();
    assert_eq!(conn.state(), LifecycleState::Closed);

    let err = conn
        .dispatch(
            &frame(FrameType::RpcExecuteRequest, 8),
            SurfacePlane::Administration,
        )
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
}

#[test]
fn admin_frame_on_application_plane_is_rejected() {
    let mut conn = Connection::new(SurfacePlane::Application);
    conn.accept_hello(&frame(FrameType::Hello, 4)).unwrap();
    conn.accept_auth(&frame(FrameType::Auth, 4)).unwrap();

    let err = conn
        .dispatch(
            &frame(FrameType::RpcExecuteRequest, 4),
            SurfacePlane::Administration,
        )
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);

    let err = conn
        .dispatch(
            &frame(FrameType::RpcExecuteRequest, 4),
            SurfacePlane::HighAvailability,
        )
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
}

#[test]
fn standard_listener_set_has_one_config_per_surface_plane() {
    let listeners = SurfaceListenerSet::standard();

    assert_eq!(
        listeners.application().surface_plane(),
        SurfacePlane::Application
    );
    assert_eq!(
        listeners.administration().surface_plane(),
        SurfacePlane::Administration
    );
    assert_eq!(
        listeners.high_availability().surface_plane(),
        SurfacePlane::HighAvailability
    );
    assert_eq!(
        listeners.monitoring().surface_plane(),
        SurfacePlane::Monitoring
    );

    for listener in [
        listeners.application(),
        listeners.administration(),
        listeners.high_availability(),
        listeners.monitoring(),
    ] {
        assert_eq!(listener.early_data_policy(), EarlyDataPolicy::Disabled);
        assert_eq!(listener.datagram_policy(), DatagramPolicy::Disabled);
    }
}

#[test]
fn listener_created_session_is_bound_to_listener_plane() {
    let listener = SurfaceListenerConfig::administration();
    let mut conn = listener.new_connection();
    assert_eq!(conn.surface_plane(), SurfacePlane::Administration);

    conn.accept_hello(&frame(FrameType::Hello, 12)).unwrap();
    conn.accept_auth(&frame(FrameType::Auth, 12)).unwrap();

    let err = conn
        .dispatch(
            &frame(FrameType::RpcExecuteRequest, 12),
            SurfacePlane::Application,
        )
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);

    let dispatch = conn
        .dispatch(
            &frame(FrameType::RpcExecuteRequest, 12),
            SurfacePlane::Administration,
        )
        .unwrap();
    assert_eq!(dispatch.frame_type, FrameType::RpcExecuteRequest);
}

#[test]
fn cancellation_routing_is_session_bound_and_drain_aware() {
    let mut conn = Connection::new(SurfacePlane::Application);
    conn.accept_hello(&frame(FrameType::Hello, 22)).unwrap();
    conn.accept_auth(&frame(FrameType::Auth, 22)).unwrap();

    let signal = CancellationSignal {
        request_id: RequestId::new(9),
        session_id: SessionId::new(22),
        cause: CancellationCause::ClientRequested,
    };
    assert_eq!(
        conn.route_cancellation(&signal).unwrap(),
        CancellationOutcome::Delivered
    );

    let wrong_session = CancellationSignal {
        session_id: SessionId::new(23),
        ..signal
    };
    assert_eq!(
        conn.route_cancellation(&wrong_session).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );

    conn.begin_drain().unwrap();
    assert_eq!(
        conn.route_cancellation(&signal).unwrap(),
        CancellationOutcome::DeliveredDuringDrain
    );

    let closed_during_drain = CancellationSignal {
        cause: CancellationCause::SessionClosed,
        ..signal
    };
    assert_eq!(
        conn.route_cancellation(&closed_during_drain)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
}
