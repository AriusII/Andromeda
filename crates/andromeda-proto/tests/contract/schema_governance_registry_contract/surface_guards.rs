use super::super::support::{
    CRATE_LOCAL_PROTO_SCHEMAS, PROTOCOL_SCHEMAS, active_schema_text, schema_contains,
    schema_identifier_tokens,
};

#[test]
fn governed_schemas_stay_message_only_without_service_rpc_sql_grpc_json_or_serde_identifiers() {
    for schema in CRATE_LOCAL_PROTO_SCHEMAS {
        let active_schema = active_schema_text(schema.source);
        for token in schema_identifier_tokens(&active_schema) {
            let lower = token.to_ascii_lowercase();
            assert_ne!(
                lower, "service",
                "{} schema must not define protobuf services",
                schema.logical_name
            );
            assert_ne!(
                lower, "rpc",
                "{} schema must not define RPC service methods",
                schema.logical_name
            );
            assert!(
                !lower.contains("grpc"),
                "{} schema must not contain active gRPC identifiers",
                schema.logical_name
            );
            assert!(
                !lower.contains("tonic"),
                "{} schema must not contain active tonic identifiers",
                schema.logical_name
            );
            assert!(
                !lower.contains("json"),
                "{} schema must not contain active JSON identifiers",
                schema.logical_name
            );
            assert!(
                !lower.contains("serde"),
                "{} schema must not contain active serde identifiers",
                schema.logical_name
            );
            assert!(
                !lower.contains("sql"),
                "{} schema must not contain active SQL identifiers",
                schema.logical_name
            );
        }
    }

    assert!(
        schema_contains(PROTOCOL_SCHEMAS, "QUIC DATAGRAM is"),
        "protocol schema should preserve the telemetry-only DATAGRAM governance note"
    );
}
