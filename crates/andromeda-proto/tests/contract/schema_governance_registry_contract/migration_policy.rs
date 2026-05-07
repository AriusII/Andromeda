use andromeda_proto::descriptor_set_bytes;
use prost::Message;
use prost_types::FileDescriptorSet;

use super::super::support::{
    CONTRACT_SCHEMAS, CRATE_LOCAL_PROTO_SCHEMAS, active_schema_text,
    assert_descriptor_messages_have_no_deprecated_fields,
};

#[test]
fn v1_migration_policy_locks_catalog_manifest_resolution_status_values() {
    let catalog_schema = CONTRACT_SCHEMAS
        .iter()
        .find(|(name, _)| *name == "catalog")
        .map(|(_, source)| *source)
        .expect("catalog contract schema must be governed");

    for required_status in [
        "STATUS_UNSPECIFIED = 0;",
        "STATUS_RESOLVED = 1;",
        "STATUS_NOT_FOUND = 2;",
        "STATUS_CATALOG_VERSION_MISMATCH = 3;",
        "STATUS_CONTRACT_HASH_MISMATCH = 4;",
        "STATUS_NOT_SOURCE_GENERATOR_READY = 5;",
        "STATUS_PERMISSION_DENIED = 6;",
        "STATUS_UNSUPPORTED = 7;",
        "STATUS_MALFORMED = 8;",
        "STATUS_INTERNAL = 9;",
        "STATUS_CATALOG_NOT_READY = 10;",
        "STATUS_AUTH_REQUIRED = 11;",
    ] {
        assert!(
            catalog_schema.contains(required_status),
            "catalog manifest resolution status migration policy missing: {required_status}"
        );
    }
}

#[test]
fn v1_migration_policy_has_no_unrecorded_deprecated_fields_or_json_mapping_options() {
    for schema in CRATE_LOCAL_PROTO_SCHEMAS {
        let active_schema = active_schema_text(schema.source);
        assert!(
            !active_schema.contains("[deprecated = true]"),
            "{} must not introduce V1 deprecated fields without a decision-record allow-list",
            schema.logical_name
        );
        assert!(
            !active_schema.contains("json_name"),
            "{} must not define protobuf JSON mapping options on the runtime contract surface",
            schema.logical_name
        );
    }

    let descriptor_set = FileDescriptorSet::decode(descriptor_set_bytes())
        .expect("generated descriptor set bytes must decode as FileDescriptorSet");
    for file in &descriptor_set.file {
        assert_descriptor_messages_have_no_deprecated_fields(
            file.name.as_deref().unwrap_or("<unnamed proto>"),
            &file.message_type,
            &mut Vec::new(),
        );
    }
}
