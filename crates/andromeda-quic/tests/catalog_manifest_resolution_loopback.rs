use andromeda_error::{AndromedaErrorKind, AndromedaResult};
use andromeda_principal::{CertificateIdentity, SurfaceScope};
use andromeda_procedure_contract::{
    ProcedureGatewayManifest as CatalogProcedureManifest,
    ProcedureGatewayProtocolLayout as CatalogProcedureProtocolLayout,
    ProcedureGatewayRequiredPermission as CatalogRequiredPermission,
};
use andromeda_proto::generated::contract::v1::catalog_procedure_manifest_resolution_request;
use andromeda_quic::catalog_manifest_resolution::CatalogManifestResolutionContext;
use andromeda_quic::{
    CatalogManifestResolutionGateway, CatalogManifestResolutionRuntime, SurfacePlane,
    TransportEndpointMetadata, TransportMessage,
};
use andromeda_rpc_codec::{
    CatalogManifestResolutionRequest, CatalogManifestResolutionResponse,
    CatalogManifestResolutionStatus, CatalogProcedureManifestResolutionRequest,
    CatalogProcedureManifestResolutionResponse, catalog_manifest_resolution_request_frame,
    decode_catalog_manifest_resolution_request_frame,
    decode_catalog_manifest_resolution_response_frame,
};
use andromeda_rpc_protocol::{FrameCodec, StreamRole};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId, SessionId, TransactionId};

fn hash_vec(byte: u8) -> Vec<u8> {
    vec![byte; ContractHash::LEN]
}

fn hash(byte: u8) -> ContractHash {
    ContractHash::test_vector(byte)
}

fn manifest() -> CatalogProcedureManifest {
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

fn request() -> CatalogProcedureManifestResolutionRequest {
    CatalogProcedureManifestResolutionRequest {
        protocol_major: 1,
        protocol_minor: 0,
        request_id: 501,
        trace_id: Some("trace-catalog-501".to_string()),
        selector: Some(
            catalog_procedure_manifest_resolution_request::Selector::ProcedureName(
                "Inventory.ReserveStock".to_string(),
            ),
        ),
        expected_contract_hash: Some(hash_vec(0x11)),
        expected_catalog_version: Some(9),
        require_source_generator_ready: true,
    }
}

fn response(
    request: &CatalogManifestResolutionRequest,
    status: CatalogManifestResolutionStatus,
    manifest: Option<CatalogProcedureManifest>,
) -> CatalogManifestResolutionResponse {
    let resolved = status == CatalogManifestResolutionStatus::Resolved;

    CatalogManifestResolutionResponse {
        protocol_major: request.protocol_major,
        protocol_minor: request.protocol_minor,
        request_id: request.request_id,
        trace_id: request.trace_id.clone(),
        status,
        manifest,
        current_catalog_version: Some(CatalogVersion::new(9)),
        diagnostic_code: (!resolved).then(|| format!("{status:?}")),
    }
}

fn metadata(subject: &str) -> TransportEndpointMetadata {
    metadata_with_surface(
        SurfacePlane::Administration,
        SurfaceScope::Administration,
        subject,
    )
}

fn metadata_with_surface(
    plane: SurfacePlane,
    scope: SurfaceScope,
    subject: &str,
) -> TransportEndpointMetadata {
    TransportEndpointMetadata::new(
        plane,
        Some(SessionId::new(601)),
        Some(CertificateIdentity::new("a".repeat(64), subject.to_string(), scope).unwrap()),
    )
}

fn metadata_without_identity(plane: SurfacePlane) -> TransportEndpointMetadata {
    TransportEndpointMetadata::new(plane, Some(SessionId::new(601)), None)
}

fn metadata_without_session(subject: &str) -> TransportEndpointMetadata {
    TransportEndpointMetadata::new(
        SurfacePlane::Administration,
        None,
        Some(
            CertificateIdentity::new(
                "a".repeat(64),
                subject.to_string(),
                SurfaceScope::Administration,
            )
            .unwrap(),
        ),
    )
}

fn request_message(
    request: &CatalogProcedureManifestResolutionRequest,
    subject: &str,
) -> TransportMessage {
    request_message_with_metadata(request, metadata(subject))
}

fn request_message_with_metadata(
    request: &CatalogProcedureManifestResolutionRequest,
    metadata: TransportEndpointMetadata,
) -> TransportMessage {
    let frame = catalog_manifest_resolution_request_frame(
        request,
        SessionId::new(601),
        Some(TransactionId::new(701)),
        hash(0x11),
        CatalogVersion::new(9),
    )
    .unwrap();

    let encoded = FrameCodec::encode(&frame).unwrap();
    let decoded = FrameCodec::decode(&encoded).unwrap();
    decode_catalog_manifest_resolution_request_frame(&decoded).unwrap();

    TransportMessage::new(metadata, StreamRole::CommandBidirectional, decoded).unwrap()
}

fn decode_response(message: TransportMessage) -> CatalogProcedureManifestResolutionResponse {
    let encoded = FrameCodec::encode(message.frame()).unwrap();
    let frame = FrameCodec::decode(&encoded).unwrap();
    decode_catalog_manifest_resolution_response_frame(&frame).unwrap()
}

#[derive(Debug, Clone)]
struct ResolvingRuntime;

impl CatalogManifestResolutionRuntime for ResolvingRuntime {
    fn resolve_catalog_manifest(
        &mut self,
        context: &CatalogManifestResolutionContext,
        request: CatalogManifestResolutionRequest,
    ) -> AndromedaResult<CatalogManifestResolutionResponse> {
        assert_eq!(context.request_id(), request.request_id);
        assert_eq!(context.session_id(), SessionId::new(601));
        assert_eq!(context.surface_plane(), SurfacePlane::Administration);
        assert_eq!(context.trace_id(), request.trace_id.as_deref());

        Ok(response(
            &request,
            CatalogManifestResolutionStatus::Resolved,
            Some(manifest()),
        ))
    }
}

#[derive(Debug, Clone)]
struct PanicRuntime;

impl CatalogManifestResolutionRuntime for PanicRuntime {
    fn resolve_catalog_manifest(
        &mut self,
        _context: &CatalogManifestResolutionContext,
        _request: CatalogManifestResolutionRequest,
    ) -> AndromedaResult<CatalogManifestResolutionResponse> {
        panic!("route admission must reject before runtime dispatch");
    }
}

#[test]
fn catalog_manifest_resolution_loopback_quic() {
    let request = request();
    let mut gateway = CatalogManifestResolutionGateway::new(ResolvingRuntime);

    let response = decode_response(
        gateway
            .route_transport_message(request_message(&request, "catalog-reader"))
            .unwrap(),
    );

    assert_eq!(
        response.status,
        CatalogManifestResolutionStatus::Resolved as i32
    );
    assert_eq!(response.request_id, request.request_id);
    assert_eq!(response.trace_id, request.trace_id);
    assert_eq!(
        response
            .manifest
            .expect("resolved response carries manifest")
            .procedure_name,
        "Inventory.ReserveStock"
    );
}

#[test]
fn catalog_manifest_resolution_rejects_application_surface_before_runtime() {
    let request = request();
    let mut gateway = CatalogManifestResolutionGateway::new(PanicRuntime);
    let message = request_message_with_metadata(
        &request,
        metadata_with_surface(
            SurfacePlane::Application,
            SurfaceScope::Application,
            "app-service",
        ),
    );

    let err = gateway.route_transport_message(message).unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(
        err.message().contains("Administration surface"),
        "route admission should reject non-Administration surface before runtime"
    );
}

#[test]
fn catalog_manifest_resolution_rejects_missing_identity_before_runtime() {
    let request = request();
    let mut gateway = CatalogManifestResolutionGateway::new(PanicRuntime);
    let message = request_message_with_metadata(
        &request,
        metadata_without_identity(SurfacePlane::Administration),
    );

    let err = gateway.route_transport_message(message).unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(
        err.message().contains("certificate identity"),
        "route admission should reject missing mTLS identity before runtime"
    );
}

#[test]
fn catalog_manifest_resolution_rejects_missing_session_before_runtime() {
    let request = request();
    let mut gateway = CatalogManifestResolutionGateway::new(PanicRuntime);
    let message =
        request_message_with_metadata(&request, metadata_without_session("catalog-reader"));

    let err = gateway.route_transport_message(message).unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(
        err.message().contains("session"),
        "route admission should reject missing authenticated session before runtime"
    );
}

#[test]
fn catalog_manifest_resolution_rejects_scope_mismatch_before_runtime() {
    let request = request();
    let mut gateway = CatalogManifestResolutionGateway::new(PanicRuntime);
    let message = request_message_with_metadata(
        &request,
        metadata_with_surface(
            SurfacePlane::Administration,
            SurfaceScope::Application,
            "app-scope-on-admin-plane",
        ),
    );

    let err = gateway.route_transport_message(message).unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(
        err.message().contains("certificate scope"),
        "route admission should reject mTLS scope mismatch before runtime"
    );
}
