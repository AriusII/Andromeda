use super::*;
use andromeda_procedure_contract::{
    ProcedureGatewayManifest, ProcedureGatewayProtocolLayout, ProcedureGatewayRequiredPermission,
};
use andromeda_rpc_codec::decode_and_validate_rpc_execute_request;
use andromeda_types::ProcedureId;

fn contract_hash(byte: u8) -> ContractHash {
    ContractHash::test_vector(byte)
}

fn route_manifest() -> ProcedureGatewayManifest {
    ProcedureGatewayManifest {
        procedure_id: ProcedureId::new(42),
        procedure_name: "Inventory.ReserveStock".to_string(),
        contract_hash: contract_hash(0x11),
        catalog_version: CatalogVersion::new(9),
        protocol_layout: ProcedureGatewayProtocolLayout {
            descriptor_set_hash: contract_hash(0x22),
            frame_envelope_hash: contract_hash(0x33),
            protocol_package: "andromeda.protocol.v1".to_string(),
            contract_package: "andromeda.contract.v1".to_string(),
        },
        result_streams: Vec::new(),
        stats_version: 5,
        policy_version: contract_hash(0x44),
        required_permissions: vec![ProcedureGatewayRequiredPermission {
            id: "andromeda.execute_procedure".to_string(),
            family: "application".to_string(),
        }],
    }
}

fn valid_generated_execute_request(
    manifest: &ProcedureGatewayManifest,
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

fn execute_request_frame_from_generated(
    request: generated::protocol::v1::RpcExecuteRequest,
    envelope_contract_hash: ContractHash,
    envelope_catalog_version: CatalogVersion,
) -> FrameBytes {
    let envelope = generated::protocol::v1::FrameEnvelope {
        protocol_version: Some(generated::protocol::v1::ProtocolVersion { major: 1, minor: 0 }),
        contract_hash: envelope_contract_hash.as_bytes().to_vec(),
        catalog_version: envelope_catalog_version.get(),
        request_id: 501,
        session_id: 601,
        tx_id: None,
        payload_kind: generated::protocol::v1::PayloadKind::RpcExecuteRequest as i32,
        payload: encode_generated_message(&request),
    };
    let payload = encode_generated_message(&envelope);

    FrameBytes {
        header: FrameHeader {
            frame_type: FrameType::RpcExecuteRequest,
            request_id: RequestId::new(501),
            session_id: SessionId::new(601),
            tx_id: None,
            payload_length: payload.len() as u64,
            flags: 0,
            header_crc: FRAME_HEADER_CRC_UNCHECKED,
        },
        payload,
    }
}

fn decode_request(
    request: generated::protocol::v1::RpcExecuteRequest,
    manifest: &ProcedureGatewayManifest,
) -> andromeda_error::AndromedaResult<andromeda_procedure_contract::ProcedureGatewayExecuteRequest>
{
    let frame = execute_request_frame_from_generated(
        request,
        manifest.contract_hash,
        manifest.catalog_version,
    );
    decode_and_validate_rpc_execute_request(&frame, manifest)
}

#[test]
fn rpc_execute_decoder_projects_manifest_bound_request() {
    let manifest = route_manifest();

    let request = decode_request(valid_generated_execute_request(&manifest), &manifest).unwrap();

    assert_eq!(request.procedure_name, manifest.procedure_name);
    assert_eq!(request.expected_contract_hash, manifest.contract_hash);
    assert_eq!(request.expected_catalog_version, manifest.catalog_version);
    assert_eq!(request.expected_stats_version, manifest.stats_version);
    assert_eq!(request.surface_scope, "application");
    assert_eq!(request.argument_count, 0);
}

#[test]
fn rpc_execute_decoder_rejects_manifest_identity_drift() {
    let manifest = route_manifest();

    let mut request = valid_generated_execute_request(&manifest);
    request.expected_contract_hash = contract_hash(0x99).as_bytes().to_vec();
    let err = decode_request(request, &manifest).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("ContractHash"));

    let mut request = valid_generated_execute_request(&manifest);
    request.expected_catalog_version = 10;
    let err = decode_request(request, &manifest).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("CatalogVersion"));

    let mut request = valid_generated_execute_request(&manifest);
    request.expected_stats_version = Some(manifest.stats_version + 1);
    let err = decode_request(request, &manifest).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("StatsVersion"));

    let mut request = valid_generated_execute_request(&manifest);
    request.procedure_name = "Inventory.ReleaseStock".to_string();
    let err = decode_request(request, &manifest).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("Procedure name"));
}

#[test]
fn rpc_execute_decoder_rejects_envelope_and_request_drift() {
    let manifest = route_manifest();
    let request = valid_generated_execute_request(&manifest);

    let frame = execute_request_frame_from_generated(
        request.clone(),
        contract_hash(0x77),
        manifest.catalog_version,
    );
    let err = decode_and_validate_rpc_execute_request(&frame, &manifest).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("envelope ContractHash"));

    let frame = execute_request_frame_from_generated(
        request,
        manifest.contract_hash,
        CatalogVersion::new(77),
    );
    let err = decode_and_validate_rpc_execute_request(&frame, &manifest).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("envelope CatalogVersion"));
}

#[test]
fn rpc_execute_decoder_rejects_missing_or_wrong_application_contract_fields() {
    let manifest = route_manifest();

    let mut request = valid_generated_execute_request(&manifest);
    request.expected_stats_version = None;
    let err = decode_request(request, &manifest).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("expected_stats_version"));

    let mut request = valid_generated_execute_request(&manifest);
    request.surface_scope = "administration".to_string();
    let err = decode_request(request, &manifest).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(err.message().contains("surface_scope"));
}

#[test]
fn rpc_execute_decoder_runs_canonical_generated_request_validation() {
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
    let err = decode_request(request, &manifest).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("argument names"));

    let mut request = valid_generated_execute_request(&manifest);
    request.arguments = vec![generated::protocol::v1::rpc_execute_request::Argument {
        name: "Quantity".to_string(),
        type_name: "i64".to_string(),
        value: Vec::new(),
    }];
    let err = decode_request(request, &manifest).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("argument value"));

    let mut request = valid_generated_execute_request(&manifest);
    request.budget = Some(
        generated::protocol::v1::rpc_execute_request::RequestBudget {
            cpu_micros: Some(5_000),
            memory_bytes: Some(64 * 1024),
            io_bytes: Some(128 * 1024),
            priority_class: Some(0),
        },
    );
    let err = decode_request(request, &manifest).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("priority_class"));
}

#[test]
fn rpc_execute_decoder_rejects_unsupported_gateway_manifest_layouts() {
    let mut manifest = route_manifest();
    manifest.protocol_layout.protocol_package = "grpc.andromeda.protocol.v1".to_string();
    let err = decode_request(valid_generated_execute_request(&manifest), &manifest).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("protocol_package"));

    let mut manifest = route_manifest();
    manifest.protocol_layout.contract_package = "grpc.andromeda.contract.v1".to_string();
    let err = decode_request(valid_generated_execute_request(&manifest), &manifest).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("contract_package"));
}
