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
    CertificateIdentity as CoreCertificateIdentity, CertificateIdentityStatus, ContractHash,
    InvocationId, Permission, PermissionSet, Principal, PrincipalAuthorizationDenialReason,
    PrincipalAuthorizationEvaluationStage, PrincipalAuthorizationEvidence,
    PrincipalAuthorizationOutcome, PrincipalBinding, PrincipalId, PrincipalRegistry, PrincipalRole,
    PrincipalStatus, ProcedureId, RequestId, SessionId, SessionToken,
    SurfaceScope as CoreSurfaceScope, TransactionId,
};
use andromeda_core::{CertificateIdentity, SurfaceScope};
use andromeda_proto::{PayloadKind, encode_generated_message, generated};
use andromeda_quic::{
    CatalogProcedureManifest, CatalogProcedureProtocolLayout, CatalogRequiredPermission,
    Connection, FRAME_HEADER_CRC_UNCHECKED, FrameBytes, FrameHeader, FrameType, LifecycleState,
    ProcedureGateway, ProcedureRouteAdmissionError, ResultStreamMetadataPolicy, SurfacePlane,
    TypedResultStreamContext,
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

fn assert_route_rejection_before_authorization(
    err: ProcedureRouteAdmissionError,
    expected_kind: AndromedaErrorKind,
    expected_message: &str,
) {
    assert_eq!(err.kind(), expected_kind);
    assert!(
        err.message().contains(expected_message),
        "pre-dispatch route rejection should contain {expected_message:?}: {}",
        err.message()
    );
    assert_eq!(
        err.authorization_denial_reason(),
        None,
        "route rejection should happen before IAM authorization is evaluated"
    );
    assert!(
        err.authorization_evidence().is_none(),
        "route rejection should not carry IAM evidence"
    );
}

fn assert_common_authorization_evidence(
    evidence: &PrincipalAuthorizationEvidence,
    expected_outcome: PrincipalAuthorizationOutcome,
    expected_reason: &str,
    expected_stage: PrincipalAuthorizationEvaluationStage,
    expected_policy_version_registry: &PrincipalRegistry,
) {
    assert_eq!(
        evidence.outcome, expected_outcome,
        "IAM evidence must be machine-classified"
    );
    assert_eq!(evidence.reason, expected_reason);
    assert_eq!(evidence.reason_code(), expected_reason);
    assert_eq!(evidence.evaluation_stage(), expected_stage);
    assert_eq!(
        evidence.policy_version,
        expected_policy_version_registry.policy_version()
    );
    assert!(evidence.has_identity_evidence());
    assert!(evidence.has_reason());
    assert!(evidence.has_policy_version());
    assert!(
        evidence.is_audit_ready(),
        "IAM evidence must be ready for durable audit binding"
    );
    let expected_policy_binding = expected_policy_version_registry
        .policy_evidence_binding()
        .expect("registry policy evidence binding should be canonical");
    assert!(
        evidence.matches_policy_version_and_digest(&expected_policy_binding),
        "IAM evidence must bind the registry policy version and digest"
    );
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
    assert_common_authorization_evidence(
        evidence,
        PrincipalAuthorizationOutcome::Denied,
        expected_reason.as_str(),
        expected_reason.evaluation_stage(),
        &registry,
    );
    assert_eq!(
        evidence.required_permission,
        Permission::ExecuteProcedure(ProcedureId::new(42))
    );
    assert_eq!(evidence.surface_scope, CoreSurfaceScope::Application);
    match expected_reason {
        PrincipalAuthorizationDenialReason::UnknownCertificate => {
            assert_eq!(
                evidence.certificate_fingerprint,
                format!("fingerprint:{}", "a".repeat(64))
            );
            assert_eq!(evidence.certificate_subject, "subject:unknown");
            assert_eq!(evidence.certificate_surface_scope, None);
            assert_eq!(evidence.certificate_status, None);
            assert_eq!(evidence.principal_id, None);
            assert_eq!(evidence.principal_status, None);
            assert!(!evidence.surface_policy_evaluated);
            assert!(!evidence.role_permission_evaluated);
            assert!(!evidence.direct_permission_evaluated);
        }
        PrincipalAuthorizationDenialReason::CertificateRevoked => {
            assert_eq!(evidence.certificate_fingerprint, "a".repeat(64));
            assert_eq!(evidence.certificate_subject, "app-service");
            assert_eq!(
                evidence.certificate_surface_scope,
                Some(CoreSurfaceScope::Application)
            );
            assert_eq!(
                evidence.certificate_status,
                Some(CertificateIdentityStatus::Revoked)
            );
            assert_eq!(evidence.principal_status, Some(PrincipalStatus::Active));
            assert!(!evidence.surface_policy_evaluated);
            assert!(!evidence.role_permission_evaluated);
            assert!(!evidence.direct_permission_evaluated);
        }
        PrincipalAuthorizationDenialReason::CertificateDisabled => {
            assert_eq!(evidence.certificate_fingerprint, "a".repeat(64));
            assert_eq!(evidence.certificate_subject, "app-service");
            assert_eq!(
                evidence.certificate_surface_scope,
                Some(CoreSurfaceScope::Application)
            );
            assert_eq!(
                evidence.certificate_status,
                Some(CertificateIdentityStatus::Disabled)
            );
            assert_eq!(evidence.principal_status, Some(PrincipalStatus::Active));
            assert!(!evidence.surface_policy_evaluated);
            assert!(!evidence.role_permission_evaluated);
            assert!(!evidence.direct_permission_evaluated);
        }
        PrincipalAuthorizationDenialReason::SurfaceScopeMismatch => {
            assert_eq!(evidence.certificate_fingerprint, "a".repeat(64));
            assert_eq!(
                evidence.certificate_surface_scope,
                Some(CoreSurfaceScope::Administration)
            );
            assert_eq!(
                evidence.certificate_status,
                Some(CertificateIdentityStatus::Active)
            );
            assert_eq!(evidence.principal_status, Some(PrincipalStatus::Active));
            assert!(!evidence.surface_policy_evaluated);
            assert!(!evidence.role_permission_evaluated);
            assert!(!evidence.direct_permission_evaluated);
        }
        PrincipalAuthorizationDenialReason::PrincipalDisabled => {
            assert_eq!(evidence.certificate_fingerprint, "a".repeat(64));
            assert_eq!(
                evidence.certificate_surface_scope,
                Some(CoreSurfaceScope::Application)
            );
            assert_eq!(
                evidence.certificate_status,
                Some(CertificateIdentityStatus::Active)
            );
            assert_eq!(evidence.principal_status, Some(PrincipalStatus::Disabled));
            assert!(!evidence.surface_policy_evaluated);
            assert!(!evidence.role_permission_evaluated);
            assert!(!evidence.direct_permission_evaluated);
        }
        PrincipalAuthorizationDenialReason::PrincipalMissingPermission => {
            assert_eq!(evidence.certificate_fingerprint, "a".repeat(64));
            assert_eq!(
                evidence.certificate_surface_scope,
                Some(CoreSurfaceScope::Application)
            );
            assert_eq!(
                evidence.certificate_status,
                Some(CertificateIdentityStatus::Active)
            );
            assert_eq!(evidence.principal_status, Some(PrincipalStatus::Active));
            assert!(evidence.surface_policy_evaluated);
            assert!(evidence.surface_policy_allowed);
            assert!(evidence.role_permission_evaluated);
            assert!(!evidence.role_permission_granted);
            assert!(evidence.direct_permission_evaluated);
            assert!(!evidence.direct_permission_granted);
        }
        PrincipalAuthorizationDenialReason::SurfaceDoesNotPermitPermission => {
            panic!("Application execute route should not produce a surface-policy denial");
        }
    }
}

#[path = "procedure_gateway_route/authorized_pre_dispatch.rs"]
mod authorized_pre_dispatch;

#[path = "procedure_gateway_route/invalid_frames.rs"]
mod invalid_frames;
#[path = "procedure_gateway_route/manifest_match.rs"]
mod manifest_match;
#[path = "procedure_gateway_route/no_sql_no_grpc.rs"]
mod no_sql_no_grpc;
#[path = "procedure_gateway_route/permission.rs"]
mod permission;
#[path = "procedure_gateway_route/route_admission.rs"]
mod route_admission;
#[path = "procedure_gateway_route/runtime_dispatch.rs"]
mod runtime_dispatch;
#[path = "procedure_gateway_route/surface_separation.rs"]
mod surface_separation;
