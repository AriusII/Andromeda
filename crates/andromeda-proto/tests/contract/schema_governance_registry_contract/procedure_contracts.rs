use andromeda_error::AndromedaErrorKind;

use super::super::support::{
    CONTRACT_SCHEMAS, PROTOCOL_SCHEMAS, assert_all_message_definitions_have_reserved_ranges,
    declared_message_names, governance_sample_manifest, schema_contains,
};

#[test]
fn governed_schemas_declare_core_procedure_contracts_and_reserved_ranges() {
    let declared_messages = declared_message_names();

    for message in [
        "FrameEnvelope",
        "ProtocolVersion",
        "RpcExecuteRequest",
        "RpcCompletion",
        "InvocationCorrelation",
        "InvocationRequest",
    ] {
        assert!(
            declared_messages.contains(message),
            "protocol schema must include {message}"
        );
    }
    for message in ["ProtocolLayout", "ProcedureManifest", "RequiredPermission"] {
        assert!(
            declared_messages.contains(message),
            "contract schema must include {message}"
        );
    }

    for field in [
        "optional uint64 request_id = 32;",
        "optional uint64 session_id = 33;",
        "optional string trace_id = 34;",
        "optional uint64 durable_lsn = 36;",
        "repeated ResultRowCountSummary result_row_counts = 37;",
        "bytes expected_contract_hash = 2;",
        "optional uint64 expected_stats_version = 7;",
        "optional uint64 stats_version = 7;",
        "optional uint64 expected_policy_version = 8;",
        "repeated Argument arguments = 5;",
        "RequestBudget budget = 6;",
    ] {
        assert!(
            schema_contains(PROTOCOL_SCHEMAS, field),
            "protocol schema missing core Procedure field: {field}"
        );
    }

    for field in [
        "bytes descriptor_set_hash = 1;",
        "bytes frame_envelope_hash = 2;",
        "bytes policy_version = 7;",
        "repeated RequiredPermission required_permissions = 8;",
        "optional uint64 stats_version = 64;",
        "message CatalogProcedureManifestResolutionRequest",
        "message CatalogProcedureManifestResolutionResponse",
        "optional bytes expected_contract_hash = 7;",
        "optional uint64 expected_catalog_version = 8;",
        "bool require_source_generator_ready = 9;",
        "ProcedureManifest manifest = 6;",
        "optional bytes resolved_contract_hash = 7;",
        "optional uint64 resolved_catalog_version = 8;",
    ] {
        assert!(
            schema_contains(CONTRACT_SCHEMAS, field),
            "contract schema missing core Procedure field: {field}"
        );
    }

    for reservation in [
        "reserved 4 to 31;",
        "reserved 38 to 63;",
        "reserved 11 to 31;",
    ] {
        assert!(
            schema_contains(PROTOCOL_SCHEMAS, reservation),
            "protocol schema missing reserved range: {reservation}"
        );
    }

    for reservation in [
        "reserved 7 to 31;",
        "reserved 9 to 31;",
        "reserved 12 to 31;",
    ] {
        assert!(
            schema_contains(CONTRACT_SCHEMAS, reservation),
            "contract schema missing reserved range: {reservation}"
        );
    }

    assert_all_message_definitions_have_reserved_ranges();
}

#[test]
fn procedure_manifest_enforces_generator_readiness_and_permission_policy_presence() {
    let manifest = governance_sample_manifest();
    assert!(manifest.validate().is_ok());
    assert!(manifest.ensure_source_generator_ready().is_ok());

    let mut zero_policy = governance_sample_manifest();
    zero_policy.policy_version = andromeda_proto::ManifestPolicyVersion::zero();
    assert!(zero_policy.validate().is_ok());
    assert_eq!(
        zero_policy
            .ensure_source_generator_ready()
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Contract
    );

    let mut no_perms = governance_sample_manifest();
    no_perms.required_permissions.clear();
    assert_eq!(
        no_perms.ensure_source_generator_ready().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );

    let mut collide = governance_sample_manifest();
    collide.protocol_layout.frame_envelope_hash = collide.protocol_layout.descriptor_set_hash;
    assert_eq!(
        collide.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );
}
