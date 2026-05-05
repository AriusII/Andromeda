//! D4 Executor Bridge Dispatch Tests
//!
//! These tests validate the contract surface between QUIC transport and executor dispatch:
//!
//! 1. **Authorization Boundary**: Certificate identity → surface scope → authorization gate
//! 2. **Cross-Plane Rejection**: Application cert attempting HA/DR operation → pre-transaction error
//! 3. **Stream Correlation**: QUIC stream ID ↔ InvocationId deterministic mapping
//! 4. **Result Stream Mapping**: Executor completion → frame encoding (contract only; actual frame
//!    encoding is deferred to D5)
//!
//! These tests are runtime-free and do not depend on quinn or rustls.

use andromeda_core::{AndromedaErrorKind, InvocationId};
use andromeda_observe::{CertificateIdentity, SurfaceScope, TraceId};
use andromeda_quic::{
    Connection, ExecutorDispatchBridge, FRAME_HEADER_CRC_UNCHECKED, FrameBytes, FrameHeader,
    FrameType, LifecycleState, SurfacePlane,
};

fn hello_frame(session_id: u64) -> FrameBytes {
    FrameBytes {
        header: FrameHeader {
            frame_type: FrameType::Hello,
            request_id: andromeda_core::RequestId::new(1),
            session_id: andromeda_core::SessionId::new(session_id),
            tx_id: None,
            payload_length: 0,
            flags: 0,
            header_crc: FRAME_HEADER_CRC_UNCHECKED,
        },
        payload: Vec::new(),
    }
}

fn auth_frame(session_id: u64) -> FrameBytes {
    FrameBytes {
        header: FrameHeader {
            frame_type: FrameType::Auth,
            request_id: andromeda_core::RequestId::new(1),
            session_id: andromeda_core::SessionId::new(session_id),
            tx_id: None,
            payload_length: 0,
            flags: 0,
            header_crc: FRAME_HEADER_CRC_UNCHECKED,
        },
        payload: Vec::new(),
    }
}

fn setup_active_application_connection() -> Connection {
    let mut conn = Connection::new(SurfacePlane::Application);
    let identity = CertificateIdentity::new(
        "a".repeat(64),
        "app-service".to_string(),
        SurfaceScope::Application,
    )
    .unwrap();
    conn.set_certificate_identity(identity).unwrap();
    conn.accept_hello(&hello_frame(100)).unwrap();
    conn.accept_auth(&auth_frame(100)).unwrap();
    assert_eq!(conn.state(), LifecycleState::Active);
    conn
}

fn setup_active_administration_connection() -> Connection {
    let mut conn = Connection::new(SurfacePlane::Administration);
    let identity = CertificateIdentity::new(
        "b".repeat(64),
        "admin-service".to_string(),
        SurfaceScope::Administration,
    )
    .unwrap();
    conn.set_certificate_identity(identity).unwrap();
    conn.accept_hello(&hello_frame(200)).unwrap();
    conn.accept_auth(&auth_frame(200)).unwrap();
    assert_eq!(conn.state(), LifecycleState::Active);
    conn
}

fn setup_active_ha_connection() -> Connection {
    let mut conn = Connection::new(SurfacePlane::HighAvailability);
    let identity = CertificateIdentity::new(
        "c".repeat(64),
        "ha-service".to_string(),
        SurfaceScope::Cluster,
    )
    .unwrap();
    conn.set_certificate_identity(identity).unwrap();
    conn.accept_hello(&hello_frame(300)).unwrap();
    conn.accept_auth(&auth_frame(300)).unwrap();
    assert_eq!(conn.state(), LifecycleState::Active);
    conn
}

/// Test 1: Bridge accepts a valid authorized invocation on Application plane.
///
/// This test validates that:
/// - Bridge construction succeeds when cert identity is bound and scope matches plane.
/// - Bridge exposes the certificate fingerprint, subject, and plane.
/// - Stream-to-invocation mapping is deterministic.
/// - No authorization error is raised for a properly set-up connection.
#[test]
fn test_bridge_accepts_authorized_invocation() {
    let conn = setup_active_application_connection();
    let bridge = ExecutorDispatchBridge::new(&conn).expect("bridge construction failed");

    // Verify bridge state.
    assert_eq!(bridge.surface_plane(), SurfacePlane::Application);
    assert_eq!(
        bridge.certificate_identity().fingerprint,
        "a".repeat(64),
        "certificate fingerprint mismatch"
    );
    assert_eq!(
        bridge.certificate_identity().subject,
        "app-service",
        "certificate subject mismatch"
    );
    assert_eq!(
        bridge.certificate_identity().surface,
        SurfaceScope::Application,
        "certificate scope mismatch"
    );

    // Verify stream mapping.
    let stream_id = 42u64;
    let invocation_id = bridge.map_stream_to_invocation_id(stream_id);
    assert_eq!(
        invocation_id,
        InvocationId::new(42),
        "stream mapping failed"
    );

    // Verify preconditions check passes for active connection.
    bridge
        .validate_dispatch_preconditions()
        .expect("dispatch preconditions validation failed");
}

/// Test 2: Bridge rejects cross-plane invocations before executor is reached.
///
/// Scenario: An Administration certificate is presented, but the connection
/// is on the Application plane. The bridge must reject this with a security error
/// *before* any executor invocation.
///
/// This test validates that:
/// - Bridge construction fails when scope does not match plane.
/// - Connection-level validation prevents scope mismatches at set_certificate_identity time.
/// - No transaction or executor invocation occurs as a result of the rejection.
#[test]
fn test_bridge_rejects_cross_plane_invocation() {
    // Try to bind an Administration identity to an Application connection.
    let mut conn = Connection::new(SurfacePlane::Application);

    let admin_identity = CertificateIdentity::new(
        "wrong_scope".repeat(8),
        "admin-service".to_string(),
        SurfaceScope::Administration,
    )
    .unwrap();

    let result = conn.set_certificate_identity(admin_identity);

    // Connection rejects the bind because scope != plane.
    assert!(result.is_err(), "connection should reject mismatched scope");
    let err = result.unwrap_err();
    assert_eq!(
        err.kind(),
        AndromedaErrorKind::Protocol,
        "error kind should be Protocol"
    );
    assert!(
        err.message().contains("surface scope"),
        "error message should mention scope"
    );

    // Verify that bridge construction would fail if we somehow got here.
    let bridge_result = ExecutorDispatchBridge::new(&conn);
    assert!(
        bridge_result.is_err(),
        "bridge should fail without certificate identity"
    );
}

/// Test 3: Bridge correctly correlates stream ID to invocation.
///
/// This test validates that:
/// - Stream ID → InvocationId mapping is deterministic.
/// - Multiple calls with the same stream_id produce the same invocation_id.
/// - Different stream_ids produce different invocation_ids.
/// - The mapping is injective (one-to-one).
#[test]
fn test_bridge_correlates_stream_id_to_invocation() {
    let conn = setup_active_application_connection();
    let bridge = ExecutorDispatchBridge::new(&conn).expect("bridge construction failed");

    // Determinism: same stream_id → same invocation_id.
    let stream_id = 12345u64;
    let inv_id_1 = bridge.map_stream_to_invocation_id(stream_id);
    let inv_id_2 = bridge.map_stream_to_invocation_id(stream_id);
    assert_eq!(inv_id_1, inv_id_2, "mapping should be deterministic");

    // Correctness: stream_id should map to InvocationId(stream_id).
    assert_eq!(
        inv_id_1,
        InvocationId::new(stream_id),
        "stream_id should map to InvocationId with same value"
    );

    // Injectivity: different stream_ids → different invocation_ids.
    let stream_id_2 = 54321u64;
    let inv_id_3 = bridge.map_stream_to_invocation_id(stream_id_2);
    assert_ne!(
        inv_id_1, inv_id_3,
        "different stream_ids should map to different invocation_ids"
    );

    // Multiple correlation roundtrips.
    for stream_id in 1..=10 {
        let inv_id = bridge.map_stream_to_invocation_id(stream_id);
        assert_eq!(
            inv_id,
            InvocationId::new(stream_id),
            "stream {} should map to InvocationId {}",
            stream_id,
            stream_id
        );
    }
}

/// Test 4: Bridge validates preconditions before invocation.
///
/// This test validates that:
/// - Preconditions check fails if connection is not in Active state.
/// - Preconditions check fails if certificate identity is not bound.
/// - Preconditions check succeeds if connection is Active and identity is bound.
///
/// This is a contract test for the admission gate: before any executor invocation,
/// the bridge must verify that the connection is ready to dispatch.
#[test]
fn test_bridge_validates_preconditions() {
    // Scenario 1: Connection not yet authenticated (not Active).
    let mut conn = Connection::new(SurfacePlane::Application);
    let identity = CertificateIdentity::new(
        "d".repeat(64),
        "test-service".to_string(),
        SurfaceScope::Application,
    )
    .unwrap();
    conn.set_certificate_identity(identity.clone()).unwrap();

    // Connection is in Hello state, not Active.
    assert_eq!(conn.state(), LifecycleState::Hello);

    let bridge = ExecutorDispatchBridge::new(&conn).expect("bridge construction succeeded");
    let precond_err = bridge.validate_dispatch_preconditions();
    assert!(
        precond_err.is_err(),
        "preconditions should fail for non-Active connection"
    );
    assert!(
        precond_err.unwrap_err().message().contains("Active"),
        "error should mention Active state"
    );

    // Scenario 2: Connection is Active.
    let conn_active = setup_active_application_connection();
    let bridge_active =
        ExecutorDispatchBridge::new(&conn_active).expect("bridge construction succeeded");

    let precond_ok = bridge_active.validate_dispatch_preconditions();
    assert!(
        precond_ok.is_ok(),
        "preconditions should succeed for Active connection with identity"
    );
}

/// Test 5: Bridge enforces plane-specific authorization boundaries.
///
/// This test validates multi-plane scenarios:
/// - Application bridge with Application identity → preconditions pass.
/// - Administration bridge with Administration identity → preconditions pass.
/// - HA bridge with Cluster identity → preconditions pass.
///
/// Each plane must have its own authorization context and must not cross-dispatch.
#[test]
fn test_bridge_enforces_plane_specific_boundaries() {
    // Application plane.
    let app_conn = setup_active_application_connection();
    let app_bridge =
        ExecutorDispatchBridge::new(&app_conn).expect("app bridge construction failed");
    assert_eq!(app_bridge.surface_plane(), SurfacePlane::Application);
    assert!(
        app_bridge.validate_dispatch_preconditions().is_ok(),
        "app bridge should validate preconditions"
    );

    // Administration plane.
    let admin_conn = setup_active_administration_connection();
    let admin_bridge =
        ExecutorDispatchBridge::new(&admin_conn).expect("admin bridge construction failed");
    assert_eq!(admin_bridge.surface_plane(), SurfacePlane::Administration);
    assert!(
        admin_bridge.validate_dispatch_preconditions().is_ok(),
        "admin bridge should validate preconditions"
    );

    // HA plane.
    let ha_conn = setup_active_ha_connection();
    let ha_bridge = ExecutorDispatchBridge::new(&ha_conn).expect("ha bridge construction failed");
    assert_eq!(ha_bridge.surface_plane(), SurfacePlane::HighAvailability);
    assert!(
        ha_bridge.validate_dispatch_preconditions().is_ok(),
        "ha bridge should validate preconditions"
    );

    // Each bridge should correlate stream IDs independently.
    let stream_id = 999u64;
    let app_inv = app_bridge.map_stream_to_invocation_id(stream_id);
    let admin_inv = admin_bridge.map_stream_to_invocation_id(stream_id);
    let ha_inv = ha_bridge.map_stream_to_invocation_id(stream_id);

    // All should map to the same InvocationId despite different planes.
    // (The mapping is stream_id-based, not plane-specific.)
    assert_eq!(app_inv, admin_inv);
    assert_eq!(admin_inv, ha_inv);
    assert_eq!(app_inv, InvocationId::new(stream_id));
}

/// Test 6: Bridge exposes immutable connection and identity references.
///
/// This test validates that:
/// - Bridge holds references, not ownership.
/// - Certificate identity is accessible but not mutated.
/// - Connection state is accessible through the bridge.
#[test]
fn test_bridge_exposes_references() {
    let conn = setup_active_application_connection();
    let bridge = ExecutorDispatchBridge::new(&conn).expect("bridge construction failed");

    // Verify that bridge references are consistent.
    let id_1 = bridge.certificate_identity();
    let id_2 = bridge.certificate_identity();
    assert_eq!(id_1.fingerprint, id_2.fingerprint);
    assert_eq!(id_1.subject, id_2.subject);

    // Verify that connection is accessible.
    let conn_ref = bridge.connection();
    assert_eq!(conn_ref.state(), LifecycleState::Active);
    assert_eq!(conn_ref.surface_plane(), SurfacePlane::Application);
}

/// Test 7: Bridge handles Monitoring plane correctly.
///
/// The Monitoring plane is read-only for diagnostics. This test validates
/// that the bridge correctly constructs and routes on the Monitoring plane.
#[test]
fn test_bridge_supports_monitoring_plane() {
    let mut conn = Connection::new(SurfacePlane::Monitoring);
    let identity = CertificateIdentity::new(
        "e".repeat(64),
        "monitoring-agent".to_string(),
        SurfaceScope::MonitoringAgent,
    )
    .unwrap();
    conn.set_certificate_identity(identity).unwrap();
    conn.accept_hello(&hello_frame(400)).unwrap();
    conn.accept_auth(&auth_frame(400)).unwrap();

    let bridge = ExecutorDispatchBridge::new(&conn).expect("monitoring bridge construction failed");

    assert_eq!(bridge.surface_plane(), SurfacePlane::Monitoring);
    assert_eq!(
        bridge.certificate_identity().surface,
        SurfaceScope::MonitoringAgent
    );
    assert!(
        bridge.validate_dispatch_preconditions().is_ok(),
        "monitoring bridge should validate preconditions"
    );
}

/// Test 8: Bridge stream correlation integrates with frame correlation.
///
/// This test validates that stream ID → InvocationId → frame correlation
/// produces consistent trace evidence. (This is a contract test; actual frame
/// encoding is deferred to D5.)
#[test]
fn test_bridge_stream_correlation_enables_frame_tracing() {
    let conn = setup_active_application_connection();
    let bridge = ExecutorDispatchBridge::new(&conn).expect("bridge construction failed");

    // Simulate QUIC frame arrival on stream 777.
    let stream_id = 777u64;
    let invocation_id = bridge.map_stream_to_invocation_id(stream_id);

    // The invocation_id should be usable as a trace correlation point.
    assert_eq!(invocation_id, InvocationId::new(stream_id));

    // Frame-level correlation can now look up the invocation by stream_id → invocation_id.
    let stream_id_again = 777u64;
    let invocation_id_again = bridge.map_stream_to_invocation_id(stream_id_again);
    assert_eq!(invocation_id, invocation_id_again);
}

/// Test 9: Bridge rejects missing certificate identity explicitly.
///
/// This test validates that the bridge catches missing identity at construction time,
/// not at dispatch time. This is important for fail-fast semantics.
#[test]
fn test_bridge_rejects_missing_certificate_identity() {
    let mut conn = Connection::new(SurfacePlane::Application);
    conn.accept_hello(&hello_frame(500)).unwrap();
    conn.accept_auth(&auth_frame(500)).unwrap();
    assert_eq!(conn.state(), LifecycleState::Active);

    // No identity bound.
    let result = ExecutorDispatchBridge::new(&conn);
    assert!(
        result.is_err(),
        "bridge should reject connection without identity"
    );

    let err = result.unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(
        err.message().contains("certificate identity"),
        "error should mention missing certificate identity"
    );
}

/// Test 10: Bridge supports multiple bridges for the same connection.
///
/// This test validates that multiple bridges can be created from the same
/// connection (they hold immutable references and do not block each other).
#[test]
fn test_bridge_allows_multiple_instances_from_same_connection() {
    let conn = setup_active_application_connection();

    let bridge_1 = ExecutorDispatchBridge::new(&conn).expect("first bridge construction failed");
    let bridge_2 = ExecutorDispatchBridge::new(&conn).expect("second bridge construction failed");

    // Both bridges should operate independently.
    assert_eq!(bridge_1.surface_plane(), bridge_2.surface_plane());
    assert_eq!(
        bridge_1.certificate_identity().fingerprint,
        bridge_2.certificate_identity().fingerprint
    );

    // Stream mapping should be consistent across bridges.
    let stream_id = 888u64;
    let inv_1 = bridge_1.map_stream_to_invocation_id(stream_id);
    let inv_2 = bridge_2.map_stream_to_invocation_id(stream_id);
    assert_eq!(inv_1, inv_2);
}
