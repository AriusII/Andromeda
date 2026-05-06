use andromeda_core::{
    AndromedaResult, CatalogVersion, ContractHash, ProcedureId, SessionId, TransactionId,
};
use andromeda_observe::{CertificateIdentity, SurfaceScope};
use andromeda_proto::generated::contract::v1::catalog_procedure_manifest_resolution_request;
use andromeda_quic::{
    CatalogManifestResolutionContext, CatalogManifestResolutionGateway,
    CatalogManifestResolutionRequest, CatalogManifestResolutionResponse,
    CatalogManifestResolutionRuntime, CatalogManifestResolutionStatus, CatalogProcedureManifest,
    CatalogProcedureManifestResolutionRequest, CatalogProcedureManifestResolutionResponse,
    CatalogProcedureProtocolLayout, CatalogRequiredPermission, FrameCodec, StreamRole,
    SurfacePlane, TransportEndpointMetadata, TransportMessage,
    catalog_manifest_resolution_request_frame, decode_catalog_manifest_resolution_request_frame,
    decode_catalog_manifest_resolution_response_frame,
};

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
    TransportEndpointMetadata::new(
        SurfacePlane::Administration,
        Some(SessionId::new(601)),
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
    let frame = catalog_manifest_resolution_request_frame(
        request,
        SessionId::new(601),
        Some(TransactionId::new(701)),
        ContractHash::zero(),
        CatalogVersion::new(9),
    )
    .unwrap();

    let encoded = FrameCodec::encode(&frame).unwrap();
    let decoded = FrameCodec::decode(&encoded).unwrap();
    decode_catalog_manifest_resolution_request_frame(&decoded).unwrap();

    TransportMessage::new(metadata(subject), StreamRole::CommandBidirectional, decoded).unwrap()
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
struct PermissionRuntime;

impl CatalogManifestResolutionRuntime for PermissionRuntime {
    fn resolve_catalog_manifest(
        &mut self,
        context: &CatalogManifestResolutionContext,
        request: CatalogManifestResolutionRequest,
    ) -> AndromedaResult<CatalogManifestResolutionResponse> {
        if context
            .certificate_identity()
            .is_some_and(|identity| identity.subject == "catalog-reader")
        {
            return Ok(response(
                &request,
                CatalogManifestResolutionStatus::Resolved,
                Some(manifest()),
            ));
        }

        Ok(response(
            &request,
            CatalogManifestResolutionStatus::PermissionDenied,
            None,
        ))
    }
}

#[derive(Debug, Clone)]
struct NotReadyRuntime;

impl CatalogManifestResolutionRuntime for NotReadyRuntime {
    fn resolve_catalog_manifest(
        &mut self,
        _context: &CatalogManifestResolutionContext,
        request: CatalogManifestResolutionRequest,
    ) -> AndromedaResult<CatalogManifestResolutionResponse> {
        Ok(response(
            &request,
            CatalogManifestResolutionStatus::CatalogNotReady,
            None,
        ))
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
    assert_eq!(response.resolved_contract_hash, Some(hash_vec(0x11)));
    assert_eq!(response.resolved_catalog_version, Some(9));
    assert_eq!(
        response
            .manifest
            .expect("resolved response carries manifest")
            .procedure_name,
        "Inventory.ReserveStock"
    );
}

#[test]
fn catalog_manifest_resolution_permission_denied() {
    let request = request();
    let mut gateway = CatalogManifestResolutionGateway::new(PermissionRuntime);

    let response = decode_response(
        gateway
            .route_transport_message(request_message(&request, "unauthorized-service"))
            .unwrap(),
    );

    assert_eq!(
        response.status,
        CatalogManifestResolutionStatus::PermissionDenied as i32
    );
    assert!(response.manifest.is_none());
    assert!(response.resolved_contract_hash.is_none());
    assert!(response.resolved_catalog_version.is_none());
    assert_eq!(response.current_catalog_version, Some(9));
}

#[test]
fn catalog_manifest_resolution_not_ready() {
    let request = request();
    let mut gateway = CatalogManifestResolutionGateway::new(NotReadyRuntime);

    let response = decode_response(
        gateway
            .route_transport_message(request_message(&request, "catalog-reader"))
            .unwrap(),
    );

    assert_eq!(
        response.status,
        CatalogManifestResolutionStatus::CatalogNotReady as i32
    );
    assert!(response.manifest.is_none());
    assert_eq!(response.current_catalog_version, Some(9));
}
