use std::{fs, path::PathBuf};

use andromeda_rpc_protocol::{
    ERROR_FRAME_CODE, FrameType, PayloadKindInvariants, RPC_BATCH_FRAME_CODE,
    RPC_COMPLETION_FRAME_CODE, RPC_EXECUTE_REQUEST_FRAME_CODE, RPC_METADATA_FRAME_CODE,
};

const PROTO_SCHEMAS: &[&str] = &[
    "../andromeda-proto/proto/andromeda/contract/v1/catalog.proto",
    "../andromeda-proto/proto/andromeda/contract/v1/contract.proto",
    "../andromeda-proto/proto/andromeda/contract/v1/manifest.proto",
    "../andromeda-proto/proto/andromeda/protocol/v1/completion.proto",
    "../andromeda-proto/proto/andromeda/protocol/v1/envelope.proto",
    "../andromeda-proto/proto/andromeda/protocol/v1/error.proto",
    "../andromeda-proto/proto/andromeda/protocol/v1/invocation.proto",
    "../andromeda-proto/proto/andromeda/protocol/v1/payload.proto",
    "../andromeda-proto/proto/andromeda/protocol/v1/protocol.proto",
    "../andromeda-proto/proto/andromeda/protocol/v1/wal.proto",
];

#[test]
fn rpc_protocol_sources_do_not_absorb_hadr_control_plane_operations() {
    let forbidden = [
        "clusterpromote",
        "clusterfence",
        "updatemanifest",
        "walshippingcontrol",
        "hadrack",
        "resync",
        "evidencequery",
    ];

    let violations = rust_sources()
        .into_iter()
        .flat_map(|path| {
            let source = fs::read_to_string(&path).expect("rust source must be readable");
            let compact = source
                .chars()
                .filter(|c| !c.is_whitespace() && *c != '_' && *c != '-')
                .collect::<String>()
                .to_ascii_lowercase();
            forbidden
                .iter()
                .filter(|term| compact.contains(**term))
                .map(|term| format!("{} contains {}", path.display(), term))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    assert!(
        violations.is_empty(),
        "andromeda-rpc-protocol must not own HA/DR control-plane operation surface:\n{}",
        violations.join("\n")
    );
}

#[test]
fn proto_schemas_remain_message_only_without_service_rpc_or_json_runtime_paths() {
    for relative in PROTO_SCHEMAS {
        let source = fs::read_to_string(crate_root().join(relative))
            .unwrap_or_else(|error| panic!("failed to read {relative}: {error}"));
        let normalized = strip_comments(&source).to_ascii_lowercase();
        let tokens = normalized
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .filter(|token| !token.is_empty())
            .collect::<Vec<_>>();

        assert!(
            !tokens.contains(&"service"),
            "{relative} must not define protobuf services"
        );
        assert!(
            !tokens.contains(&"rpc"),
            "{relative} must not define protobuf RPC methods"
        );
        assert!(
            !normalized.contains("grpc")
                && !normalized.contains("tonic")
                && !normalized.contains("json")
                && !normalized.contains("serde_json"),
            "{relative} must not introduce gRPC/JSON native protocol paths"
        );
    }
}

#[test]
fn payload_kind_lockstep_remains_bounded_without_hadr_specific_frame_codes() {
    assert_eq!(*PayloadKindInvariants::LOCKED_RANGE.start(), 1);
    assert_eq!(*PayloadKindInvariants::LOCKED_RANGE.end(), 9);
    assert_eq!(
        FrameType::RpcExecuteRequest.wire_code(),
        RPC_EXECUTE_REQUEST_FRAME_CODE
    );
    assert_eq!(FrameType::RpcMetadata.wire_code(), RPC_METADATA_FRAME_CODE);
    assert_eq!(FrameType::RpcBatch.wire_code(), RPC_BATCH_FRAME_CODE);
    assert_eq!(
        FrameType::RpcCompletion.wire_code(),
        RPC_COMPLETION_FRAME_CODE
    );
    assert_eq!(FrameType::Error.wire_code(), ERROR_FRAME_CODE);
}

fn rust_sources() -> Vec<PathBuf> {
    fs::read_dir(crate_root().join("src"))
        .expect("andromeda-rpc-protocol src directory must be readable")
        .map(|entry| {
            entry
                .expect("source directory entry must be readable")
                .path()
        })
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect()
}

fn strip_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(prefix, _)| prefix))
        .collect::<Vec<_>>()
        .join("\n")
}

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
