//! Procedure gateway route contract tests.
//!
//! Rename note: this file is the governance successor to the former
//! `procedure_gateway_dispatch.rs` test name. The test body remains focused on
//! route admission and pre-dispatch authorization; no production behavior is changed.
//!
//! These tests validate the QUIC-side contract surface before Procedure dispatch:
//!
//! 1. **Authorization Boundary**: Certificate identity → surface scope → authorization gate
//! 2. **Cross-Plane Rejection**: Application cert attempting HA/DR operation → pre-transaction error
//! 3. **Stream Correlation**: QUIC stream ID ↔ InvocationId deterministic mapping
//! 4. **Result Stream Mapping**: Executor completion → frame encoding (contract only; actual frame
//!    encoding is deferred to D5)
//!
//! These tests are runtime-free and do not depend on quinn or rustls.

use andromeda_core::{
    AndromedaErrorKind, CatalogVersion, CertificateFingerprint,
    CertificateIdentity as CoreCertificateIdentity, ContractHash, InvocationId, Permission,
    PermissionSet, Principal, PrincipalAuthorizationDenialReason, PrincipalAuthorizationOutcome,
    PrincipalBinding, PrincipalId, PrincipalRegistry, PrincipalRole, PrincipalStatus, ProcedureId,
    RequestId, SessionId, SessionToken, SurfaceScope as CoreSurfaceScope, TransactionId,
};
use andromeda_observe::{CertificateIdentity, SurfaceScope};
use andromeda_proto::{PayloadKind, encode_generated_message, generated};
use andromeda_quic::{
    CatalogProcedureManifest, CatalogProcedureProtocolLayout, CatalogRequiredPermission,
    Connection, FRAME_HEADER_CRC_UNCHECKED, FrameBytes, FrameHeader, FrameType, LifecycleState,
    ProcedureGateway, ResultStreamMetadataPolicy, SurfacePlane, TypedResultStreamContext,
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

fn hash(byte: u8) -> ContractHash {
    ContractHash::test_vector(byte)
}

fn route_manifest() -> CatalogProcedureManifest {
    CatalogProcedureManifest {
        procedure_id: ProcedureId::new(42),
        procedure_name: "Inventory.ReserveStock".to_string(),
        contract_hash: hash(0x11),
        catalog_version: CatalogVersion::new(9),
        protocol_layout: CatalogProcedureProtocolLayout {
            descriptor_set_hash: hash(0x22),
            frame_envelope_hash: hash(0x33),
            protocol_package: "andromeda.protocol.v1".to_string(),
            contract_package: "andromeda.contract.v1".to_string(),
        },
        result_streams: Vec::new(),
        stats_version: 5,
        policy_version: hash(0x44),
        required_permissions: vec![CatalogRequiredPermission {
            id: "andromeda.execute_procedure".to_string(),
            family: "application".to_string(),
        }],
    }
}

fn execute_request_frame(
    procedure_name: &str,
    request_contract_hash: ContractHash,
    request_catalog_version: CatalogVersion,
    request_stats_version: Option<u64>,
    surface_scope: &str,
    envelope_contract_hash: ContractHash,
    envelope_catalog_version: CatalogVersion,
) -> FrameBytes {
    let execute_request = generated::protocol::v1::RpcExecuteRequest {
        procedure_name: procedure_name.to_string(),
        expected_contract_hash: request_contract_hash.as_bytes().to_vec(),
        expected_catalog_version: request_catalog_version.get(),
        surface_scope: surface_scope.to_string(),
        arguments: Vec::new(),
        budget: None,
        expected_stats_version: request_stats_version,
    };
    execute_request_frame_from_generated(
        execute_request,
        envelope_contract_hash,
        envelope_catalog_version,
    )
}

fn execute_request_frame_from_generated(
    execute_request: generated::protocol::v1::RpcExecuteRequest,
    envelope_contract_hash: ContractHash,
    envelope_catalog_version: CatalogVersion,
) -> FrameBytes {
    let envelope = generated::protocol::v1::FrameEnvelope {
        protocol_version: Some(generated::protocol::v1::ProtocolVersion { major: 1, minor: 0 }),
        contract_hash: envelope_contract_hash.as_bytes().to_vec(),
        catalog_version: envelope_catalog_version.get(),
        request_id: 501,
        session_id: 100,
        tx_id: None,
        payload_kind: PayloadKind::RpcExecuteRequest.wire_code() as i32,
        payload: encode_generated_message(&execute_request),
    };
    let payload = encode_generated_message(&envelope);

    FrameBytes {
        header: FrameHeader {
            frame_type: FrameType::RpcExecuteRequest,
            request_id: RequestId::new(501),
            session_id: SessionId::new(100),
            tx_id: None,
            payload_length: payload.len() as u64,
            flags: 0,
            header_crc: FRAME_HEADER_CRC_UNCHECKED,
        },
        payload,
    }
}

fn valid_generated_execute_request(
    manifest: &CatalogProcedureManifest,
) -> generated::protocol::v1::RpcExecuteRequest {
    generated::protocol::v1::RpcExecuteRequest {
        procedure_name: manifest.procedure_name.clone(),
        expected_contract_hash: manifest.contract_hash.as_bytes().to_vec(),
        expected_catalog_version: manifest.catalog_version.get(),
        surface_scope: "application".to_string(),
        arguments: Vec::new(),
        budget: None,
        expected_stats_version: Some(manifest.stats_version),
    }
}

fn valid_execute_frame(manifest: &CatalogProcedureManifest) -> FrameBytes {
    execute_request_frame(
        &manifest.procedure_name,
        manifest.contract_hash,
        manifest.catalog_version,
        Some(manifest.stats_version),
        "application",
        manifest.contract_hash,
        manifest.catalog_version,
    )
}

fn principal_with_role(
    fingerprint: &str,
    role: PrincipalRole,
    status: PrincipalStatus,
) -> (Principal, PrincipalId) {
    let cert_fingerprint = CertificateFingerprint::new(fingerprint).expect("valid fingerprint");
    let principal_id =
        PrincipalId::from_certificate_fingerprint(&cert_fingerprint).expect("principal id");
    let session_token = SessionToken::from_certificate_fingerprint(&cert_fingerprint);
    let principal =
        Principal::new_with_status(principal_id, role, status, session_token, cert_fingerprint)
            .expect("principal evidence should be valid");

    (principal, principal_id)
}

fn registry_with_binding(
    fingerprint: &str,
    certificate_scope: CoreSurfaceScope,
    role: PrincipalRole,
    status: PrincipalStatus,
    direct_permissions: PermissionSet,
) -> (PrincipalRegistry, PrincipalId) {
    let certificate = CoreCertificateIdentity::new(fingerprint, "app-service", certificate_scope)
        .expect("certificate evidence should be valid");
    let (principal, principal_id) = principal_with_role(fingerprint, role, status);
    let binding =
        PrincipalBinding::new_with_direct_permissions(certificate, principal, direct_permissions)
            .expect("valid principal binding");
    let mut registry = PrincipalRegistry::new();
    registry.register(binding).expect("registry insert");

    (registry, principal_id)
}

fn registry_for_application_user() -> (PrincipalRegistry, PrincipalId) {
    registry_with_binding(
        &"a".repeat(64),
        CoreSurfaceScope::Application,
        PrincipalRole::User,
        PrincipalStatus::Active,
        PermissionSet::new(),
    )
}

fn assert_authorized_route_denial(
    registry: PrincipalRegistry,
    expected_reason: PrincipalAuthorizationDenialReason,
) {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = valid_execute_frame(&manifest);

    let err = gateway
        .bind_authorized_application_procedure_route(42, &frame, &manifest, &registry)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(
        err.message().contains(expected_reason.as_str()),
        "authorization denial should include the stable reason"
    );
    assert_eq!(
        err.authorization_denial_reason(),
        Some(expected_reason),
        "authorization denial should expose the typed IAM reason"
    );
    let evidence = err
        .authorization_evidence()
        .expect("IAM denial should carry authorization evidence");
    assert_eq!(
        evidence.outcome,
        PrincipalAuthorizationOutcome::Denied,
        "denial evidence must be machine-classified"
    );
    assert_eq!(evidence.reason, expected_reason.as_str());
    assert_eq!(
        evidence.required_permission,
        Permission::ExecuteProcedure(ProcedureId::new(42))
    );
    assert_eq!(evidence.surface_scope, CoreSurfaceScope::Application);
    assert!(
        evidence.has_identity_evidence(),
        "audit evidence must preserve certificate/principal identity context"
    );
}

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
        gateway.certificate_identity().fingerprint,
        "a".repeat(64),
        "certificate fingerprint mismatch"
    );
    assert_eq!(
        gateway.certificate_identity().subject,
        "app-service",
        "certificate subject mismatch"
    );
    assert_eq!(
        gateway.certificate_identity().surface,
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
/// - Administration gateway with Administration identity -> preconditions pass.
/// - HA gateway with Cluster identity -> preconditions pass.
///
/// Each plane must have its own authorization context and must not cross-dispatch.
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
    let admin_gateway =
        ProcedureGateway::new(&admin_conn).expect("admin gateway construction failed");
    assert_eq!(admin_gateway.surface_plane(), SurfacePlane::Administration);
    assert!(
        admin_gateway.validate_dispatch_preconditions().is_ok(),
        "admin gateway should validate preconditions"
    );

    // HA plane.
    let ha_conn = setup_active_ha_connection();
    let ha_gateway = ProcedureGateway::new(&ha_conn).expect("ha gateway construction failed");
    assert_eq!(ha_gateway.surface_plane(), SurfacePlane::HighAvailability);
    assert!(
        ha_gateway.validate_dispatch_preconditions().is_ok(),
        "ha gateway should validate preconditions"
    );

    // Each gateway should correlate stream IDs independently.
    let stream_id = 999u64;
    let app_inv = app_gateway.map_stream_to_invocation_id(stream_id);
    let admin_inv = admin_gateway.map_stream_to_invocation_id(stream_id);
    let ha_inv = ha_gateway.map_stream_to_invocation_id(stream_id);

    // All should map to the same InvocationId despite different planes.
    // (The mapping is stream_id-based, not plane-specific.)
    assert_eq!(app_inv, admin_inv);
    assert_eq!(admin_inv, ha_inv);
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
    assert_eq!(id_1.fingerprint, id_2.fingerprint);
    assert_eq!(id_1.subject, id_2.subject);

    // Verify that connection is accessible.
    let conn_ref = gateway.connection();
    assert_eq!(conn_ref.state(), LifecycleState::Active);
    assert_eq!(conn_ref.surface_plane(), SurfacePlane::Application);
}

///
/// The Monitoring plane is read-only for diagnostics. This test validates
/// that the gateway correctly constructs and routes on the Monitoring plane.
#[test]
fn test_gateway_supports_monitoring_plane() {
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

    let gateway = ProcedureGateway::new(&conn).expect("monitoring gateway construction failed");

    assert_eq!(gateway.surface_plane(), SurfacePlane::Monitoring);
    assert_eq!(
        gateway.certificate_identity().surface,
        SurfaceScope::MonitoringAgent
    );
    assert!(
        gateway.validate_dispatch_preconditions().is_ok(),
        "monitoring gateway should validate preconditions"
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
        gateway_1.certificate_identity().fingerprint,
        gateway_2.certificate_identity().fingerprint
    );

    // Stream mapping should be consistent across gateways.
    let stream_id = 888u64;
    let inv_1 = gateway_1.map_stream_to_invocation_id(stream_id);
    let inv_2 = gateway_2.map_stream_to_invocation_id(stream_id);
    assert_eq!(inv_1, inv_2);
}

#[test]
fn test_gateway_binds_application_execute_route_to_manifest_before_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        manifest.catalog_version,
        Some(manifest.stats_version),
        "application",
        manifest.contract_hash,
        manifest.catalog_version,
    );

    let binding = gateway
        .bind_application_procedure_route(42, &frame, &manifest)
        .expect("procedure route binding should validate");

    assert_eq!(binding.invocation_id, InvocationId::new(42));
    assert_eq!(binding.request_id, RequestId::new(501));
    assert_eq!(binding.session_id, SessionId::new(100));
    assert_eq!(binding.surface_plane, SurfacePlane::Application);
    assert_eq!(binding.procedure_id, manifest.procedure_id);
    assert_eq!(binding.contract_hash, manifest.contract_hash);
    assert_eq!(binding.catalog_version, manifest.catalog_version);
    assert_eq!(binding.stats_version, manifest.stats_version);
    assert_eq!(
        binding.execute_request.procedure_name,
        "Inventory.ReserveStock"
    );
    assert_eq!(
        binding.execute_request.expected_contract_hash,
        manifest.contract_hash
    );
    assert_eq!(
        binding.execute_request.expected_catalog_version,
        manifest.catalog_version
    );
    assert_eq!(
        binding.execute_request.expected_stats_version,
        manifest.stats_version
    );
    assert_eq!(
        binding.certificate_identity.surface,
        SurfaceScope::Application
    );
    assert_eq!(
        binding.typed_result_stream_context(),
        TypedResultStreamContext::new(
            RequestId::new(501),
            SessionId::new(100),
            None,
            manifest.contract_hash,
            manifest.catalog_version,
        )
    );
    let result_dispatch =
        binding.result_stream_dispatch_policy(ResultStreamMetadataPolicy::RowBatchRequired);
    assert_eq!(
        result_dispatch.typed_result_stream_context(),
        Some(binding.typed_result_stream_context()),
        "gateway-created result dispatch policy must carry admitted route context"
    );
}

#[test]
fn test_gateway_rejects_non_application_surface_before_procedure_dispatch() {
    let conn = setup_active_administration_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        manifest.catalog_version,
        Some(manifest.stats_version),
        "application",
        manifest.contract_hash,
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(7, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(
        err.message().contains("Application surface"),
        "wrong-surface error should name the Application surface"
    );
}

#[test]
fn test_gateway_rejects_contract_mismatch_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = execute_request_frame(
        "Inventory.ReserveStock",
        hash(0x99),
        manifest.catalog_version,
        Some(manifest.stats_version),
        "application",
        hash(0x99),
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(8, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("ContractHash"),
        "contract mismatch error should name ContractHash"
    );
}

#[test]
fn test_gateway_rejects_surface_scope_mismatch_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        manifest.catalog_version,
        Some(manifest.stats_version),
        "administration",
        manifest.contract_hash,
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(9, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(
        err.message().contains("surface_scope"),
        "surface mismatch error should name the protobuf surface_scope"
    );
}

#[test]
fn test_gateway_rejects_catalog_version_mismatch_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        CatalogVersion::new(10),
        Some(manifest.stats_version),
        "application",
        manifest.contract_hash,
        CatalogVersion::new(10),
    );

    let err = gateway
        .bind_application_procedure_route(10, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("CatalogVersion"),
        "catalog version mismatch error should name CatalogVersion"
    );
}

#[test]
fn test_gateway_rejects_stats_version_mismatch_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        manifest.catalog_version,
        Some(manifest.stats_version + 1),
        "application",
        manifest.contract_hash,
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(11, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("StatsVersion"),
        "stats version mismatch error should name StatsVersion"
    );
}

#[test]
fn test_gateway_rejects_missing_stats_version_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        manifest.catalog_version,
        None,
        "application",
        manifest.contract_hash,
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(12, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("expected_stats_version"),
        "missing stats version error should name expected_stats_version"
    );
}

#[test]
fn test_gateway_rejects_duplicate_execute_argument_names_before_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let mut request = valid_generated_execute_request(&manifest);
    request.arguments = vec![
        generated::protocol::v1::rpc_execute_request::Argument {
            name: "Quantity".to_string(),
            type_name: "i64".to_string(),
            value: 3_i64.to_le_bytes().to_vec(),
        },
        generated::protocol::v1::rpc_execute_request::Argument {
            name: "Quantity".to_string(),
            type_name: "i64".to_string(),
            value: 4_i64.to_le_bytes().to_vec(),
        },
    ];
    let frame = execute_request_frame_from_generated(
        request,
        manifest.contract_hash,
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(18, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("argument names"),
        "canonical execute request validation should reject duplicate argument names"
    );
}

#[test]
fn test_gateway_rejects_empty_execute_argument_value_before_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let mut request = valid_generated_execute_request(&manifest);
    request.arguments = vec![generated::protocol::v1::rpc_execute_request::Argument {
        name: "Quantity".to_string(),
        type_name: "i64".to_string(),
        value: Vec::new(),
    }];
    let frame = execute_request_frame_from_generated(
        request,
        manifest.contract_hash,
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(19, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("argument value"),
        "canonical execute request validation should reject empty argument values"
    );
}

#[test]
fn test_gateway_rejects_zero_execute_budget_priority_before_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let mut request = valid_generated_execute_request(&manifest);
    request.budget = Some(
        generated::protocol::v1::rpc_execute_request::RequestBudget {
            cpu_micros: Some(5_000),
            memory_bytes: Some(64 * 1024),
            io_bytes: Some(128 * 1024),
            priority_class: Some(0),
        },
    );
    let frame = execute_request_frame_from_generated(
        request,
        manifest.contract_hash,
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(20, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("priority_class"),
        "canonical execute request validation should reject zero budget priority"
    );
}

#[test]
fn test_gateway_rejects_zero_manifest_stats_version_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let mut manifest = route_manifest();
    manifest.stats_version = 0;
    let frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        manifest.catalog_version,
        Some(5),
        "application",
        manifest.contract_hash,
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(17, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("stats_version") || err.message().contains("stats version"),
        "manifest binding error should name StatsVersion"
    );
}

#[test]
fn test_gateway_rejects_procedure_name_mismatch_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = execute_request_frame(
        "Inventory.ReleaseStock",
        manifest.contract_hash,
        manifest.catalog_version,
        Some(manifest.stats_version),
        "application",
        manifest.contract_hash,
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(13, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("Procedure name"),
        "procedure name mismatch error should name Procedure name"
    );
}

#[test]
fn test_gateway_rejects_envelope_contract_mismatch_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        manifest.catalog_version,
        Some(manifest.stats_version),
        "application",
        hash(0x77),
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(14, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("envelope ContractHash"),
        "envelope mismatch error should name envelope ContractHash"
    );
}

#[test]
fn test_gateway_rejects_envelope_catalog_mismatch_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        manifest.catalog_version,
        Some(manifest.stats_version),
        "application",
        manifest.contract_hash,
        CatalogVersion::new(77),
    );

    let err = gateway
        .bind_application_procedure_route(15, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("envelope CatalogVersion"),
        "envelope mismatch error should name envelope CatalogVersion"
    );
}

#[test]
fn test_gateway_rejects_client_transaction_id_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let mut frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        manifest.catalog_version,
        Some(manifest.stats_version),
        "application",
        manifest.contract_hash,
        manifest.catalog_version,
    );
    frame.header.tx_id = Some(TransactionId::new(700));

    let err = gateway
        .bind_application_procedure_route(16, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    assert!(
        err.message().contains("client transaction id"),
        "client tx_id rejection should name client transaction id"
    );
}

#[test]
fn test_gateway_rejects_frame_envelope_context_mismatch_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let mut frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        manifest.catalog_version,
        Some(manifest.stats_version),
        "application",
        manifest.contract_hash,
        manifest.catalog_version,
    );
    frame.header.request_id = RequestId::new(502);

    let err = gateway
        .bind_application_procedure_route(18, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    assert!(
        err.message().contains("context"),
        "route must reject frame/envelope context divergence"
    );
}

#[test]
fn test_gateway_rejects_frame_session_not_bound_to_connection_before_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let mut frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        manifest.catalog_version,
        Some(manifest.stats_version),
        "application",
        manifest.contract_hash,
        manifest.catalog_version,
    );
    frame.header.session_id = SessionId::new(101);

    let err = gateway
        .bind_application_procedure_route(20, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    assert!(
        err.message().contains("authenticated connection"),
        "route must reject frames from a different authenticated session"
    );
}

#[test]
fn test_gateway_rejects_manifest_without_execute_permission_before_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let mut manifest = route_manifest();
    manifest.required_permissions.clear();
    let frame = valid_execute_frame(&manifest);

    let err = gateway
        .bind_application_procedure_route(19, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("required_permissions"),
        "manifest permission rejection should name required_permissions"
    );
    assert!(
        err.message().contains("andromeda.execute_procedure"),
        "manifest permission rejection should name the required execute permission"
    );
}

#[test]
fn test_gateway_authorized_route_allows_core_principal_before_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = valid_execute_frame(&manifest);
    let (registry, principal_id) = registry_for_application_user();

    let authorized = gateway
        .bind_authorized_application_procedure_route(42, &frame, &manifest, &registry)
        .expect("core IAM should allow the route before dispatch");

    assert_eq!(authorized.route.invocation_id, InvocationId::new(42));
    assert_eq!(authorized.route.procedure_id, ProcedureId::new(42));
    assert_eq!(authorized.principal_id, principal_id);
    assert_eq!(
        authorized.authorization_evidence.outcome,
        PrincipalAuthorizationOutcome::Allowed
    );
    assert_eq!(authorized.authorization_evidence.reason, "allowed");
    assert_eq!(
        authorized.authorization_evidence.required_permission,
        Permission::ExecuteProcedure(ProcedureId::new(42))
    );
    assert_eq!(
        authorized.authorization_evidence.surface_scope,
        CoreSurfaceScope::Application
    );
    assert!(authorized.authorization_evidence.surface_policy_evaluated);
    assert!(authorized.authorization_evidence.surface_policy_allowed);
    assert!(authorized.authorization_evidence.role_permission_evaluated);
    assert!(authorized.authorization_evidence.role_permission_granted);
    assert!(
        authorized
            .authorization_evidence
            .direct_permission_evaluated
    );
}

#[test]
fn test_gateway_authorized_route_rejects_unknown_certificate_before_dispatch() {
    assert_authorized_route_denial(
        PrincipalRegistry::new(),
        PrincipalAuthorizationDenialReason::UnknownCertificate,
    );
}

#[test]
fn test_gateway_authorized_route_rejects_revoked_certificate_before_dispatch() {
    let (mut registry, _) = registry_for_application_user();
    registry
        .revoke_certificate(&"a".repeat(64))
        .expect("registered certificate can be revoked");

    assert_authorized_route_denial(
        registry,
        PrincipalAuthorizationDenialReason::CertificateRevoked,
    );
}

#[test]
fn test_gateway_authorized_route_rejects_disabled_principal_before_dispatch() {
    let (registry, _) = registry_with_binding(
        &"a".repeat(64),
        CoreSurfaceScope::Application,
        PrincipalRole::User,
        PrincipalStatus::Disabled,
        PermissionSet::new(),
    );

    assert_authorized_route_denial(
        registry,
        PrincipalAuthorizationDenialReason::PrincipalDisabled,
    );
}

#[test]
fn test_gateway_authorized_route_rejects_core_surface_scope_mismatch_before_dispatch() {
    let (registry, _) = registry_with_binding(
        &"a".repeat(64),
        CoreSurfaceScope::Administration,
        PrincipalRole::User,
        PrincipalStatus::Active,
        PermissionSet::new(),
    );

    assert_authorized_route_denial(
        registry,
        PrincipalAuthorizationDenialReason::SurfaceScopeMismatch,
    );
}

#[test]
fn test_gateway_authorized_route_rejects_missing_execute_permission_before_dispatch() {
    let (registry, _) = registry_with_binding(
        &"a".repeat(64),
        CoreSurfaceScope::Application,
        PrincipalRole::Guest,
        PrincipalStatus::Active,
        PermissionSet::new(),
    );

    assert_authorized_route_denial(
        registry,
        PrincipalAuthorizationDenialReason::PrincipalMissingPermission,
    );
}
