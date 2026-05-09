use super::*;

const HADR_STREAM_MIN: u64 = 128;
const HADR_STREAM_MAX: u64 = 255;

///
/// This test validates that:
/// - Gateway construction succeeds when cert identity is bound and scope matches plane.
/// - Gateway exposes the certificate fingerprint, subject, and plane.
/// - Stream-to-invocation mapping is deterministic.
/// - No authorization error is raised for a properly set-up connection.
#[test]
fn test_gateway_accepts_authorized_invocation() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");

    // Verify gateway state.
    assert_eq!(gateway.surface_plane(), SurfacePlane::Application);
    assert_eq!(
        gateway.certificate_identity().fingerprint().as_str(),
        "a".repeat(64),
        "certificate fingerprint mismatch"
    );
    assert_eq!(
        gateway.certificate_identity().subject(),
        "app-service",
        "certificate subject mismatch"
    );
    assert_eq!(
        gateway.certificate_identity().surface_scope(),
        SurfaceScope::Application,
        "certificate scope mismatch"
    );

    // Verify stream mapping.
    let stream_id = 42u64;
    let invocation_id = gateway.map_stream_to_invocation_id(stream_id);
    assert_eq!(
        invocation_id,
        InvocationId::new(42),
        "stream mapping failed"
    );

    // Verify preconditions check passes for active connection.
    gateway
        .validate_dispatch_preconditions()
        .expect("dispatch preconditions validation failed");
}

///
/// Scenario: An Administration certificate is presented, but the connection
/// is on the Application plane. The gateway must reject this with a security error
/// *before* any executor invocation.
///
/// This test validates that:
/// - Gateway construction fails when scope does not match plane.
/// - Connection-level validation prevents scope mismatches at set_certificate_identity time.
/// - No transaction or executor invocation occurs as a result of the rejection.
#[test]
fn test_gateway_rejects_cross_plane_invocation() {
    // Try to bind an Administration identity to an Application connection.
    let mut conn = Connection::new(SurfacePlane::Application);

    let admin_identity = CertificateIdentity::new(
        "d".repeat(64),
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

    // Verify that gateway construction would fail if we somehow got here.
    let gateway_result = ProcedureGateway::new(&conn);
    assert!(
        gateway_result.is_err(),
        "gateway should fail without certificate identity"
    );
}

///
/// This test validates that:
/// - Stream ID → InvocationId mapping is deterministic.
/// - Multiple calls with the same stream_id produce the same invocation_id.
/// - Different stream_ids produce different invocation_ids.
/// - The mapping is injective (one-to-one).
#[test]
fn test_gateway_correlates_stream_id_to_invocation() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");

    // Determinism: same stream_id → same invocation_id.
    let stream_id = 12345u64;
    let inv_id_1 = gateway.map_stream_to_invocation_id(stream_id);
    let inv_id_2 = gateway.map_stream_to_invocation_id(stream_id);
    assert_eq!(inv_id_1, inv_id_2, "mapping should be deterministic");

    // Correctness: stream_id should map to InvocationId(stream_id).
    assert_eq!(
        inv_id_1,
        InvocationId::new(stream_id),
        "stream_id should map to InvocationId with same value"
    );

    // Injectivity: different stream_ids → different invocation_ids.
    let stream_id_2 = 54321u64;
    let inv_id_3 = gateway.map_stream_to_invocation_id(stream_id_2);
    assert_ne!(
        inv_id_1, inv_id_3,
        "different stream_ids should map to different invocation_ids"
    );

    // Multiple correlation roundtrips.
    for stream_id in 1..=10 {
        let inv_id = gateway.map_stream_to_invocation_id(stream_id);
        assert_eq!(
            inv_id,
            InvocationId::new(stream_id),
            "stream {} should map to InvocationId {}",
            stream_id,
            stream_id
        );
    }
}

///
/// This test validates that:
/// - Preconditions check fails if connection is not in Active state.
/// - Preconditions check fails if certificate identity is not bound.
/// - Preconditions check succeeds if connection is Active and identity is bound.
///
/// This is a contract test for the admission gate: before any executor invocation,
/// the gateway must verify that the connection is ready to dispatch.
#[test]
fn test_gateway_validates_preconditions() {
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

    let gateway = ProcedureGateway::new(&conn).expect("gateway construction succeeded");
    let precond_err = gateway.validate_dispatch_preconditions();
    assert!(
        precond_err.is_err(),
        "preconditions should fail for non-Active connection"
    );
    assert!(
        precond_err.unwrap_err().message().contains("Active"),
        "error should mention Active state"
    );

    let conn_active = setup_active_application_connection();
    let gateway_active =
        ProcedureGateway::new(&conn_active).expect("gateway construction succeeded");

    let precond_ok = gateway_active.validate_dispatch_preconditions();
    assert!(
        precond_ok.is_ok(),
        "preconditions should succeed for Active connection with identity"
    );
}

///
/// This test validates multi-plane scenarios:
/// - Application gateway with Application identity -> preconditions pass.
/// - Non-Application planes cannot construct a ProcedureGateway.
#[test]
fn test_gateway_enforces_plane_specific_boundaries() {
    // Application plane.
    let app_conn = setup_active_application_connection();
    let app_gateway = ProcedureGateway::new(&app_conn).expect("app gateway construction failed");
    assert_eq!(app_gateway.surface_plane(), SurfacePlane::Application);
    assert!(
        app_gateway.validate_dispatch_preconditions().is_ok(),
        "app gateway should validate preconditions"
    );

    // Administration plane.
    let admin_conn = setup_active_administration_connection();
    assert!(
        ProcedureGateway::new(&admin_conn)
            .unwrap_err()
            .message()
            .contains("Application-surface only"),
        "admin plane must not construct the Application ProcedureGateway"
    );

    // HA plane.
    let ha_conn = setup_active_ha_connection();
    assert!(
        ProcedureGateway::new(&ha_conn)
            .unwrap_err()
            .message()
            .contains("Application-surface only"),
        "HA/DR plane must not construct the Application ProcedureGateway"
    );

    let stream_id = 999u64;
    let app_inv = app_gateway.map_stream_to_invocation_id(stream_id);

    assert_eq!(app_inv, InvocationId::new(stream_id));
}

///
/// This test validates that:
/// - Gateway holds references, not ownership.
/// - Certificate identity is accessible but not mutated.
/// - Connection state is accessible through the gateway.
#[test]
fn test_gateway_exposes_references() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");

    // Verify that gateway references are consistent.
    let id_1 = gateway.certificate_identity();
    let id_2 = gateway.certificate_identity();
    assert_eq!(id_1.fingerprint(), id_2.fingerprint());
    assert_eq!(id_1.subject(), id_2.subject());

    // Verify that connection is accessible.
    let conn_ref = gateway.connection();
    assert_eq!(conn_ref.state(), LifecycleState::Active);
    assert_eq!(conn_ref.surface_plane(), SurfacePlane::Application);
}

///
/// The Monitoring plane is read-only for diagnostics. This test validates
/// that the ProcedureGateway does not construct on the Monitoring plane.
#[test]
fn test_gateway_rejects_monitoring_plane() {
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

    let err = ProcedureGateway::new(&conn).unwrap_err();

    assert!(
        err.message().contains("Application-surface only"),
        "monitoring plane must not construct the Application ProcedureGateway: {}",
        err.message()
    );
}

///
/// This test validates that stream ID → InvocationId → frame correlation
/// produces consistent trace evidence. (This is a contract test; actual frame
/// encoding is deferred to D5.)
#[test]
fn test_gateway_stream_correlation_enables_frame_tracing() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");

    // Simulate QUIC frame arrival on stream 777.
    let stream_id = 777u64;
    let invocation_id = gateway.map_stream_to_invocation_id(stream_id);

    // The invocation_id should be usable as a trace correlation point.
    assert_eq!(invocation_id, InvocationId::new(stream_id));

    // Frame-level correlation can now look up the invocation by stream_id → invocation_id.
    let stream_id_again = 777u64;
    let invocation_id_again = gateway.map_stream_to_invocation_id(stream_id_again);
    assert_eq!(invocation_id, invocation_id_again);
}

///
/// This test validates that the gateway catches missing identity at construction time,
/// not at dispatch time. This is important for fail-fast semantics.
#[test]
fn test_gateway_rejects_missing_certificate_identity() {
    let mut conn = Connection::new(SurfacePlane::Application);
    conn.accept_hello(&hello_frame(500)).unwrap();
    conn.accept_auth(&auth_frame(500)).unwrap();
    assert_eq!(conn.state(), LifecycleState::Active);

    // No identity bound.
    let result = ProcedureGateway::new(&conn);
    assert!(
        result.is_err(),
        "gateway should reject connection without identity"
    );

    let err = result.unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(
        err.message().contains("certificate identity"),
        "error should mention missing certificate identity"
    );
}

///
/// This test validates that multiple gateways can be created from the same
/// connection (they hold immutable references and do not block each other).
#[test]
fn test_gateway_allows_multiple_instances_from_same_connection() {
    let conn = setup_active_application_connection();

    let gateway_1 = ProcedureGateway::new(&conn).expect("first gateway construction failed");
    let gateway_2 = ProcedureGateway::new(&conn).expect("second gateway construction failed");

    // Both gateways should operate independently.
    assert_eq!(gateway_1.surface_plane(), gateway_2.surface_plane());
    assert_eq!(
        gateway_1.certificate_identity().fingerprint(),
        gateway_2.certificate_identity().fingerprint()
    );

    // Stream mapping should be consistent across gateways.
    let stream_id = 888u64;
    let inv_1 = gateway_1.map_stream_to_invocation_id(stream_id);
    let inv_2 = gateway_2.map_stream_to_invocation_id(stream_id);
    assert_eq!(inv_1, inv_2);
}

#[test]
fn test_gateway_rejects_non_application_surface_before_procedure_dispatch() {
    let conn = setup_active_administration_connection();
    let err = ProcedureGateway::new(&conn).unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(
        err.message().contains("Application-surface only"),
        "wrong-surface error should name the Application surface"
    );
}

#[test]
fn test_gateway_rejects_every_non_application_surface_before_procedure_dispatch() {
    let cases = [
        (
            setup_active_administration_connection(),
            SurfacePlane::Administration,
        ),
        (setup_active_ha_connection(), SurfacePlane::HighAvailability),
        (
            setup_active_monitoring_connection(),
            SurfacePlane::Monitoring,
        ),
    ];

    for (conn, plane) in cases {
        let err = ProcedureGateway::new(&conn).unwrap_err();

        assert_eq!(
            err.kind(),
            AndromedaErrorKind::Security,
            "{plane:?} must be rejected before Procedure dispatch"
        );
        assert!(
            err.message().contains("Application-surface only"),
            "{plane:?} wrong-surface error should name the Application surface: {}",
            err.message()
        );
    }
}

#[test]
fn test_application_route_rejects_non_application_request_scopes_before_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();

    for disallowed_scope in ["administration", "cluster", "monitoring"] {
        let frame = execute_request_frame(
            "Inventory.ReserveStock",
            manifest.contract_hash,
            manifest.catalog_version,
            Some(manifest.stats_version),
            disallowed_scope,
            manifest.contract_hash,
            manifest.catalog_version,
        );

        let err = gateway
            .bind_application_procedure_route(7, &frame, &manifest)
            .unwrap_err();

        assert_eq!(
            err.kind(),
            AndromedaErrorKind::Security,
            "Application route must reject {disallowed_scope} before Procedure dispatch"
        );
        assert!(
            err.message().contains("surface_scope"),
            "{disallowed_scope} rejection should identify the request surface_scope: {}",
            err.message()
        );
        assert!(
            err.message().contains("Application surface"),
            "{disallowed_scope} rejection should name the Application surface: {}",
            err.message()
        );
    }
}

#[test]
fn test_application_route_rejects_drain_before_payload_decode_or_authorization() {
    let mut conn = setup_active_application_connection();
    conn.begin_drain().expect("active connection should drain");
    assert_eq!(conn.state(), LifecycleState::Draining);

    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let mut frame = valid_execute_frame(&manifest);
    frame.payload.clear();
    frame.header.payload_length = 0;

    let err = gateway
        .bind_application_procedure_route(7, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    assert!(
        err.message().contains("Draining"),
        "drain rejection should happen before invalid payload is decoded: {}",
        err.message()
    );
    assert!(
        err.message().contains("not Active"),
        "drain rejection should name the dispatch precondition: {}",
        err.message()
    );

    let (registry, _) = registry_for_application_user();
    let err = gateway
        .bind_authorized_application_procedure_route(7, &frame, &manifest, &registry)
        .unwrap_err();
    assert_route_rejection_before_authorization(err, AndromedaErrorKind::Protocol, "Draining");
}

#[test]
fn test_application_route_rejects_admin_and_hadr_manifest_permissions_before_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");

    for (family, id) in [
        ("administration", "andromeda.admin.drain"),
        ("cluster", "andromeda.hadr.promote"),
    ] {
        let mut manifest = route_manifest();
        manifest.required_permissions = vec![CatalogRequiredPermission {
            id: id.to_string(),
            family: family.to_string(),
        }];
        let frame = valid_execute_frame(&manifest);

        let err = gateway
            .bind_application_procedure_route(7, &frame, &manifest)
            .unwrap_err();

        assert_eq!(
            err.kind(),
            AndromedaErrorKind::Contract,
            "{family}/{id} must be rejected by manifest admission"
        );
        assert!(
            err.message().contains("non-Application permission"),
            "Application route should reject privileged manifest permissions before dispatch: {}",
            err.message()
        );
    }
}

#[test]
fn application_route_accepts_only_v0_application_stream_namespace_before_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = valid_execute_frame(&manifest);
    let expected_application_range = format!("[0..={}]", HADR_STREAM_MIN - 1);
    let expected_hadr_range = format!("[{HADR_STREAM_MIN}..={HADR_STREAM_MAX}]");

    for stream_id in [0, HADR_STREAM_MIN - 1] {
        let binding = gateway
            .bind_application_procedure_route(stream_id, &frame, &manifest)
            .expect("V0 Application stream ID should be Application-routable");
        assert_eq!(
            binding.invocation_id,
            InvocationId::new(stream_id),
            "Application stream ID {stream_id} should preserve stream correlation"
        );
    }

    for stream_id in HADR_STREAM_MIN..=HADR_STREAM_MAX {
        let err = gateway
            .bind_application_procedure_route(stream_id, &frame, &manifest)
            .expect_err("HA/DR reserved stream ID must be rejected before Procedure dispatch");

        assert_eq!(
            err.kind(),
            AndromedaErrorKind::Security,
            "HA/DR reserved stream ID {stream_id} must be a surface-separation rejection"
        );
        assert!(
            err.message().contains("HA/DR reserved stream id"),
            "rejection should identify the HA/DR reserved stream namespace: {}",
            err.message()
        );
        assert!(
            err.message().contains(&expected_hadr_range),
            "rejection should report the canonical reserved range: {}",
            err.message()
        );
    }

    for stream_id in [HADR_STREAM_MAX + 1, 512, u64::MAX] {
        let err = gateway
            .bind_application_procedure_route(stream_id, &frame, &manifest)
            .expect_err("future/reserved stream ID must be rejected before Procedure dispatch");

        assert_eq!(
            err.kind(),
            AndromedaErrorKind::Security,
            "future/reserved stream ID {stream_id} must be a fail-closed rejection"
        );
        assert!(
            err.message().contains("future/reserved stream id"),
            "rejection should identify the future/reserved stream namespace: {}",
            err.message()
        );
        assert!(
            err.message().contains(&expected_application_range),
            "rejection should report the V0 Application stream range: {}",
            err.message()
        );
    }
}

fn setup_active_monitoring_connection() -> Connection {
    let mut conn = Connection::new(SurfacePlane::Monitoring);
    let identity = CertificateIdentity::new(
        "f".repeat(64),
        "monitoring-agent".to_string(),
        SurfaceScope::MonitoringAgent,
    )
    .unwrap();
    conn.set_certificate_identity(identity).unwrap();
    conn.accept_hello(&hello_frame(900)).unwrap();
    conn.accept_auth(&auth_frame(900)).unwrap();
    assert_eq!(conn.state(), LifecycleState::Active);
    conn
}
