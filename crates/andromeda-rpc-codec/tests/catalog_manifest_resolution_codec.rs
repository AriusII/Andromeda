use andromeda_error::AndromedaErrorKind;
use andromeda_procedure_contract::{
    ProcedureGatewayManifest as CatalogProcedureManifest,
    ProcedureGatewayProtocolLayout as CatalogProcedureProtocolLayout,
    ProcedureGatewayRequiredPermission as CatalogRequiredPermission,
};
use andromeda_proto::generated::contract::v1::catalog_procedure_manifest_resolution_request;
use andromeda_rpc_codec::{
    CatalogManifestResolutionRequest, CatalogManifestResolutionResponse,
    CatalogManifestResolutionStatus, CatalogProcedureManifestResolutionRequest,
    catalog_manifest_resolution_request_frame, decode_catalog_manifest_resolution_request_frame,
    decode_catalog_manifest_resolution_response_frame,
    decode_catalog_manifest_resolution_route_request,
    encode_catalog_manifest_resolution_response_frame,
};
use andromeda_rpc_protocol::FrameCodec;
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

fn request_frame() -> andromeda_rpc_protocol::FrameBytes {
    let frame = catalog_manifest_resolution_request_frame(
        &request(),
        SessionId::new(601),
        Some(TransactionId::new(701)),
        hash(0x11),
        CatalogVersion::new(9),
    )
    .unwrap();

    let encoded = FrameCodec::encode(&frame).unwrap();
    let decoded = FrameCodec::decode(&encoded).unwrap();
    decode_catalog_manifest_resolution_request_frame(&decoded).unwrap();
    decoded
}

#[test]
fn catalog_manifest_resolution_codec_loopback_projects_manifest_response() {
    let frame = request_frame();
    let decoded_request = decode_catalog_manifest_resolution_route_request(&frame).unwrap();
    let route_request = decoded_request.request.clone();
    let route_response = response(
        &route_request,
        CatalogManifestResolutionStatus::Resolved,
        Some(manifest()),
    );

    route_response
        .validate_against_request(&route_request)
        .expect("resolved response must match selector and expectations");
    let response_frame = encode_catalog_manifest_resolution_response_frame(
        &decoded_request.envelope,
        frame.header,
        &route_response,
    )
    .unwrap();
    let response = decode_catalog_manifest_resolution_response_frame(&response_frame).unwrap();

    assert_eq!(
        response.status,
        CatalogManifestResolutionStatus::Resolved as i32
    );
    assert_eq!(response.request_id, request().request_id);
    assert_eq!(response.trace_id, request().trace_id);
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
fn catalog_manifest_resolution_codec_encodes_non_resolved_status_without_manifest_binding() {
    let frame = request_frame();
    let decoded_request = decode_catalog_manifest_resolution_route_request(&frame).unwrap();
    let route_response = response(
        &decoded_request.request,
        CatalogManifestResolutionStatus::PermissionDenied,
        None,
    );

    let response_frame = encode_catalog_manifest_resolution_response_frame(
        &decoded_request.envelope,
        frame.header,
        &route_response,
    )
    .unwrap();
    let response = decode_catalog_manifest_resolution_response_frame(&response_frame).unwrap();

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
fn catalog_manifest_resolution_response_rejects_resolved_manifest_mismatch() {
    let decoded_request =
        decode_catalog_manifest_resolution_route_request(&request_frame()).unwrap();
    let mut mismatched = manifest();
    mismatched.contract_hash = hash(0x77);
    let route_response = response(
        &decoded_request.request,
        CatalogManifestResolutionStatus::Resolved,
        Some(mismatched),
    );

    let err = route_response
        .validate_against_request(&decoded_request.request)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("request expectation"));
}

#[test]
fn catalog_manifest_resolution_response_rejects_zero_manifest_binding_versions() {
    let route_request = CatalogManifestResolutionRequest::from_protobuf(request()).unwrap();

    let mut zero_stats = manifest();
    zero_stats.stats_version = 0;
    let err = response(
        &route_request,
        CatalogManifestResolutionStatus::Resolved,
        Some(zero_stats),
    )
    .to_protobuf()
    .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("stats_version"));

    let mut zero_policy = manifest();
    zero_policy.policy_version = ContractHash::zero();
    let err = response(
        &route_request,
        CatalogManifestResolutionStatus::Resolved,
        Some(zero_policy),
    )
    .to_protobuf()
    .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("policy_version"));
}
