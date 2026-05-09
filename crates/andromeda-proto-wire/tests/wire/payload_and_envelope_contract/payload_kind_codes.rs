use andromeda_error::AndromedaErrorKind;
use andromeda_proto_wire::{
    AUTH_WIRE_CODE, CONTRACT_REQUEST_WIRE_CODE, CONTRACT_RESPONSE_WIRE_CODE, ERROR_WIRE_CODE,
    HELLO_WIRE_CODE, PAYLOAD_KIND_TRANSPORT_CODE_LOCKSTEP, PayloadFrameFamily, PayloadKind,
    RPC_BATCH_WIRE_CODE, RPC_COMPLETION_WIRE_CODE, RPC_EXECUTE_REQUEST_WIRE_CODE,
    RPC_METADATA_WIRE_CODE,
};

const PROTO_WIRE_MANIFEST: &str = include_str!("../../../Cargo.toml");
const RPC_PROTOCOL_MANIFEST: &str = include_str!("../../../../andromeda-rpc-protocol/Cargo.toml");

#[test]
fn payload_kind_wire_codes_are_contract_locked() {
    let expected = [
        (
            PayloadKind::Hello,
            HELLO_WIRE_CODE,
            PayloadFrameFamily::SessionControl,
        ),
        (
            PayloadKind::Auth,
            AUTH_WIRE_CODE,
            PayloadFrameFamily::SessionControl,
        ),
        (
            PayloadKind::ContractRequest,
            CONTRACT_REQUEST_WIRE_CODE,
            PayloadFrameFamily::ContractControl,
        ),
        (
            PayloadKind::ContractResponse,
            CONTRACT_RESPONSE_WIRE_CODE,
            PayloadFrameFamily::ContractControl,
        ),
        (
            PayloadKind::RpcExecuteRequest,
            RPC_EXECUTE_REQUEST_WIRE_CODE,
            PayloadFrameFamily::RpcCommand,
        ),
        (
            PayloadKind::RpcMetadata,
            RPC_METADATA_WIRE_CODE,
            PayloadFrameFamily::RpcResultStream,
        ),
        (
            PayloadKind::RpcBatch,
            RPC_BATCH_WIRE_CODE,
            PayloadFrameFamily::RpcResultStream,
        ),
        (
            PayloadKind::RpcCompletion,
            RPC_COMPLETION_WIRE_CODE,
            PayloadFrameFamily::RpcResultStream,
        ),
        (
            PayloadKind::Error,
            ERROR_WIRE_CODE,
            PayloadFrameFamily::Diagnostic,
        ),
    ];

    assert_eq!(PAYLOAD_KIND_TRANSPORT_CODE_LOCKSTEP.len(), expected.len());

    for (kind, code, family) in expected {
        assert_eq!(kind.wire_code(), code);
        assert_eq!(PayloadKind::try_from(code).unwrap(), kind);
        assert_eq!(kind.frame_mapping().transport_frame_code, code);
        assert_eq!(kind.frame_mapping().family, family);
        assert!(
            PAYLOAD_KIND_TRANSPORT_CODE_LOCKSTEP
                .iter()
                .any(|(locked_kind, locked_code)| *locked_kind == kind && *locked_code == code)
        );
    }

    assert_eq!(
        PayloadKind::try_from(0).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );
    assert_eq!(
        PayloadKind::try_from(10).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );
    assert_eq!(
        PayloadKind::try_from(100).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn generated_payload_kind_codes_match_custom_contract_codes() {
    use andromeda_proto_wire::GeneratedPayloadKind;

    let expected = [
        (GeneratedPayloadKind::Hello, HELLO_WIRE_CODE),
        (GeneratedPayloadKind::Auth, AUTH_WIRE_CODE),
        (
            GeneratedPayloadKind::ContractRequest,
            CONTRACT_REQUEST_WIRE_CODE,
        ),
        (
            GeneratedPayloadKind::ContractResponse,
            CONTRACT_RESPONSE_WIRE_CODE,
        ),
        (
            GeneratedPayloadKind::RpcExecuteRequest,
            RPC_EXECUTE_REQUEST_WIRE_CODE,
        ),
        (GeneratedPayloadKind::RpcMetadata, RPC_METADATA_WIRE_CODE),
        (GeneratedPayloadKind::RpcBatch, RPC_BATCH_WIRE_CODE),
        (
            GeneratedPayloadKind::RpcCompletion,
            RPC_COMPLETION_WIRE_CODE,
        ),
        (GeneratedPayloadKind::Error, ERROR_WIRE_CODE),
    ];

    for (generated_kind, code) in expected {
        assert_eq!(generated_kind as u32, code);
    }
}

#[test]
fn proto_wire_keeps_payload_kind_and_frame_code_lockstep() {
    let expected = [
        (PayloadKind::Hello, HELLO_WIRE_CODE),
        (PayloadKind::Auth, AUTH_WIRE_CODE),
        (PayloadKind::ContractRequest, CONTRACT_REQUEST_WIRE_CODE),
        (PayloadKind::ContractResponse, CONTRACT_RESPONSE_WIRE_CODE),
        (
            PayloadKind::RpcExecuteRequest,
            RPC_EXECUTE_REQUEST_WIRE_CODE,
        ),
        (PayloadKind::RpcMetadata, RPC_METADATA_WIRE_CODE),
        (PayloadKind::RpcBatch, RPC_BATCH_WIRE_CODE),
        (PayloadKind::RpcCompletion, RPC_COMPLETION_WIRE_CODE),
        (PayloadKind::Error, ERROR_WIRE_CODE),
    ];

    for (kind, code) in expected {
        assert_eq!(kind.wire_code(), code);
    }
}

#[test]
fn split_manifests_reject_json_and_grpc_runtime_dependencies() {
    let manifests = [
        (
            "andromeda-proto-wire",
            PROTO_WIRE_MANIFEST.to_ascii_lowercase(),
        ),
        (
            "andromeda-rpc-protocol",
            RPC_PROTOCOL_MANIFEST.to_ascii_lowercase(),
        ),
    ];
    let forbidden = [
        "grpc",
        "grpcio",
        "json-rpc",
        "jsonrpc",
        "jsonrpsee",
        "prost-grpc",
        "prost_grpc",
        "serde_json",
        "tonic",
    ];

    for (crate_name, manifest) in manifests {
        let violations = forbidden
            .iter()
            .filter(|term| manifest.contains(**term))
            .copied()
            .collect::<Vec<_>>();
        assert!(
            violations.is_empty(),
            "{crate_name} manifest introduced forbidden JSON/gRPC runtime dependencies: {:?}",
            violations
        );
    }
}
