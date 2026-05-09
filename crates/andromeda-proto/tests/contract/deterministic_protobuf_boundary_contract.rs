use std::collections::BTreeSet;

use andromeda_procedure_contract::{ManifestPolicyVersion, ProtocolLayout};
use andromeda_proto::{
    CONTRACT_PACKAGE, DESCRIPTOR_SET_HASH_ALGORITHM, PROTOCOL_PACKAGE, descriptor_set_bytes,
    descriptor_set_hash, frame_envelope_hash, generated, protocol_layout,
};
use andromeda_types::ContractHash;
use prost::Message;
use prost_types::FileDescriptorSet;

use super::support::{
    CRATE_LOCAL_PROTO_SCHEMAS, GENERATED_WRAPPER, bytes_contains, governance_sample_manifest,
    stable_source_name_hash,
};

#[test]
fn generated_wrapper_preserves_constants_and_split_descriptor_paths() {
    assert_eq!(
        DESCRIPTOR_SET_HASH_ALGORITHM,
        "andromeda-stable-fnv1a-256-v1"
    );

    for descriptor_path in CRATE_LOCAL_PROTO_SCHEMAS.iter().map(|schema| {
        schema
            .relative_path
            .strip_prefix("proto/")
            .unwrap()
            .as_bytes()
    }) {
        assert!(
            bytes_contains(descriptor_set_bytes(), descriptor_path),
            "generated descriptor set must preserve crate-local proto path {}",
            String::from_utf8_lossy(descriptor_path)
        );
    }
}

#[test]
fn v1_migration_policy_keeps_generated_wrapper_in_descriptor_lockstep() {
    for required_snippet in [
        "include_bytes!(concat!(env!(\"OUT_DIR\"), \"/andromeda_descriptor.bin\"))",
        "include!(concat!(env!(\"OUT_DIR\"), \"/andromeda.contract.v1.rs\"))",
        "include!(concat!(env!(\"OUT_DIR\"), \"/andromeda.protocol.v1.rs\"))",
        "pub fn descriptor_set_hash() -> ContractHash",
        "pub fn frame_envelope_hash() -> ContractHash",
        "pub fn protocol_layout() -> ProtocolLayout",
        "DESCRIPTOR_SET_HASH_ALGORITHM",
    ] {
        assert!(
            GENERATED_WRAPPER.contains(required_snippet),
            "generated.rs must preserve descriptor/module/hash lockstep snippet: {required_snippet}"
        );
    }
}

#[test]
fn generated_descriptor_set_matches_crate_local_proto_sources() {
    let descriptor_set = FileDescriptorSet::decode(descriptor_set_bytes())
        .expect("generated descriptor set bytes must decode as FileDescriptorSet");
    let descriptor_files = descriptor_set
        .file
        .iter()
        .map(|file| {
            file.name
                .as_deref()
                .unwrap_or_else(|| panic!("generated descriptor file missing name: {file:?}"))
        })
        .collect::<BTreeSet<_>>();
    let governed_descriptor_paths = CRATE_LOCAL_PROTO_SCHEMAS
        .iter()
        .map(|schema| schema.relative_path.strip_prefix("proto/").unwrap())
        .collect::<BTreeSet<_>>();

    assert_eq!(
        descriptor_files, governed_descriptor_paths,
        "generated FileDescriptorSet must represent every crate-local proto source exactly once"
    );

    for schema in CRATE_LOCAL_PROTO_SCHEMAS {
        let descriptor_name = schema.relative_path.strip_prefix("proto/").unwrap();
        let descriptor = descriptor_set
            .file
            .iter()
            .find(|file| file.name.as_deref() == Some(descriptor_name))
            .unwrap_or_else(|| panic!("descriptor set missing {descriptor_name}"));

        assert_eq!(
            descriptor.package.as_deref(),
            Some(schema.expected_package),
            "{descriptor_name} descriptor package drifted from crate-local source"
        );
        assert_eq!(
            descriptor.syntax.as_deref(),
            Some("proto3"),
            "{descriptor_name} descriptor syntax must remain proto3"
        );
        assert!(
            descriptor.service.is_empty(),
            "{descriptor_name} descriptor must remain message-only and contain no services"
        );
    }

    let source_name_hash = stable_source_name_hash(
        governed_descriptor_paths.iter().copied(),
        b"andromeda-crate-local-proto-source-names",
    );
    let descriptor_name_hash = stable_source_name_hash(
        descriptor_files.iter().copied(),
        b"andromeda-crate-local-proto-source-names",
    );

    assert!(!source_name_hash.is_zero());
    assert!(!descriptor_name_hash.is_zero());
    assert_eq!(
        source_name_hash.as_bytes(),
        descriptor_name_hash.as_bytes(),
        "crate-local source-name hash must match generated descriptor file-name hash"
    );
}

#[test]
fn generated_prost_modules_follow_governed_package_layout() {
    let protocol_version = generated::protocol::v1::ProtocolVersion { major: 1, minor: 0 };
    let envelope = generated::protocol::v1::FrameEnvelope {
        protocol_version: Some(protocol_version),
        contract_hash: vec![7; ContractHash::LEN],
        catalog_version: 1,
        request_id: 2,
        session_id: 3,
        tx_id: None,
        payload_kind: generated::protocol::v1::PayloadKind::RpcBatch as i32,
        payload: b"row".to_vec(),
    };
    let manifest = generated::contract::v1::ProcedureManifest {
        procedure_id: 42,
        procedure_name: "Inventory.ReserveStock".to_string(),
        contract_hash: vec![9; ContractHash::LEN],
        catalog_version: 1,
        protocol_layout: Some(generated::contract::v1::ProtocolLayout {
            descriptor_set_hash: descriptor_set_hash().as_bytes().to_vec(),
            frame_envelope_hash: frame_envelope_hash().as_bytes().to_vec(),
            protocol_package: PROTOCOL_PACKAGE.to_string(),
            contract_package: CONTRACT_PACKAGE.to_string(),
        }),
        result_streams: Vec::new(),
        policy_version: vec![0x22; ContractHash::LEN],
        required_permissions: vec![generated::contract::v1::RequiredPermission {
            id: "andromeda.execute_procedure".to_string(),
            family: "application".to_string(),
        }],
        stats_version: Some(5),
    };
    let invocation = generated::protocol::v1::InvocationRequest {
        correlation: Some(generated::protocol::v1::InvocationCorrelation {
            request_id: Some(2),
            session_id: Some(3),
            trace_id: Some("trace-1".to_string()),
            contract_hash: Some(vec![7; ContractHash::LEN]),
            catalog_version: Some(1),
            invocation_id: None,
            stats_version: Some(5),
            expected_policy_version: Some(11),
        }),
        execute_request: Some(generated::protocol::v1::RpcExecuteRequest {
            procedure_name: "Inventory.ReserveStock".to_string(),
            expected_contract_hash: vec![7; ContractHash::LEN],
            expected_catalog_version: 1,
            surface_scope: "inventory".to_string(),
            arguments: Vec::new(),
            budget: None,
            expected_stats_version: Some(5),
        }),
    };

    assert_eq!(envelope.payload_kind, 7);
    assert_eq!(
        manifest.protocol_layout.unwrap().protocol_package,
        PROTOCOL_PACKAGE
    );
    assert_eq!(
        generated::protocol::v1::PayloadKind::RpcCompletion as i32,
        8
    );
    assert!(invocation.correlation.unwrap().request_id.is_some());
}

#[test]
fn descriptor_hashes_are_nonzero_stable_and_fit_protocol_layout() {
    let descriptor_hash = descriptor_set_hash();
    let frame_hash = frame_envelope_hash();
    let layout = protocol_layout();

    assert!(!descriptor_set_bytes().is_empty());
    assert!(!descriptor_hash.is_zero());
    assert!(!frame_hash.is_zero());
    assert_ne!(
        descriptor_hash, frame_hash,
        "descriptor_set_hash and frame_envelope_hash must be domain-separated"
    );
    assert_eq!(descriptor_hash, descriptor_set_hash());
    assert_eq!(frame_hash, frame_envelope_hash());
    assert_eq!(layout.descriptor_set_hash, descriptor_hash);
    assert_eq!(layout.frame_envelope_hash, frame_hash);
    assert!(layout.validate().is_ok());
}

#[test]
fn procedure_manifest_hash_is_deterministic_and_distinct_from_descriptor_hashes() {
    let manifest = governance_sample_manifest();
    let hash = manifest.manifest_hash();

    assert_eq!(hash, governance_sample_manifest().manifest_hash());
    assert_ne!(hash, descriptor_set_hash());
    assert_ne!(hash, frame_envelope_hash());
    assert_ne!(hash, manifest.contract_hash);
    assert!(!hash.is_zero());

    let mut bumped = governance_sample_manifest();
    bumped.policy_version = ManifestPolicyVersion::test_vector(0x99);
    assert_ne!(hash, bumped.manifest_hash());

    let mut bumped_stats = governance_sample_manifest();
    bumped_stats.stats_version += 1;
    assert_ne!(hash, bumped_stats.manifest_hash());
}

#[test]
fn protocol_layout_type_remains_the_deterministic_boundary() {
    let layout: ProtocolLayout = protocol_layout();

    assert_eq!(layout.descriptor_set_hash, descriptor_set_hash());
    assert_eq!(layout.frame_envelope_hash, frame_envelope_hash());
    assert!(layout.validate().is_ok());
}
