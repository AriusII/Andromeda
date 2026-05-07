const CARGO_MANIFEST: &str = include_str!("../../Cargo.toml");

#[test]
fn cargo_manifest_rejects_grpc_and_json_runtime_stack_drift() {
    let manifest = CARGO_MANIFEST.to_ascii_lowercase();
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

    let violations = forbidden
        .into_iter()
        .filter(|term| manifest.contains(term))
        .collect::<Vec<_>>();

    assert!(
        violations.is_empty(),
        "andromeda-quic manifest introduced forbidden RPC or JSON runtime dependencies: {:?}",
        violations
    );
}
