use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

struct ProtoSchema {
    logical_name: &'static str,
    relative_path: &'static str,
    expected_package: &'static str,
    source: &'static str,
}

const PROTOCOL_PACKAGE_DECLARATION: &str = "andromeda.protocol.v1";
const CONTRACT_PACKAGE_DECLARATION: &str = "andromeda.contract.v1";

const CRATE_LOCAL_PROTO_SCHEMAS: &[ProtoSchema] = &[
    ProtoSchema {
        logical_name: "protocol/protocol",
        relative_path: "proto/andromeda/protocol/v1/protocol.proto",
        expected_package: PROTOCOL_PACKAGE_DECLARATION,
        source: include_str!("../../proto/andromeda/protocol/v1/protocol.proto"),
    },
    ProtoSchema {
        logical_name: "protocol/envelope",
        relative_path: "proto/andromeda/protocol/v1/envelope.proto",
        expected_package: PROTOCOL_PACKAGE_DECLARATION,
        source: include_str!("../../proto/andromeda/protocol/v1/envelope.proto"),
    },
    ProtoSchema {
        logical_name: "protocol/payload",
        relative_path: "proto/andromeda/protocol/v1/payload.proto",
        expected_package: PROTOCOL_PACKAGE_DECLARATION,
        source: include_str!("../../proto/andromeda/protocol/v1/payload.proto"),
    },
    ProtoSchema {
        logical_name: "protocol/completion",
        relative_path: "proto/andromeda/protocol/v1/completion.proto",
        expected_package: PROTOCOL_PACKAGE_DECLARATION,
        source: include_str!("../../proto/andromeda/protocol/v1/completion.proto"),
    },
    ProtoSchema {
        logical_name: "protocol/error",
        relative_path: "proto/andromeda/protocol/v1/error.proto",
        expected_package: PROTOCOL_PACKAGE_DECLARATION,
        source: include_str!("../../proto/andromeda/protocol/v1/error.proto"),
    },
    ProtoSchema {
        logical_name: "protocol/invocation",
        relative_path: "proto/andromeda/protocol/v1/invocation.proto",
        expected_package: PROTOCOL_PACKAGE_DECLARATION,
        source: include_str!("../../proto/andromeda/protocol/v1/invocation.proto"),
    },
    ProtoSchema {
        logical_name: "contract/contract",
        relative_path: "proto/andromeda/contract/v1/contract.proto",
        expected_package: CONTRACT_PACKAGE_DECLARATION,
        source: include_str!("../../proto/andromeda/contract/v1/contract.proto"),
    },
    ProtoSchema {
        logical_name: "contract/manifest",
        relative_path: "proto/andromeda/contract/v1/manifest.proto",
        expected_package: CONTRACT_PACKAGE_DECLARATION,
        source: include_str!("../../proto/andromeda/contract/v1/manifest.proto"),
    },
];

const PROTOCOL_SCHEMAS: &[(&str, &str)] = &[
    (
        "protocol",
        include_str!("../../proto/andromeda/protocol/v1/protocol.proto"),
    ),
    (
        "envelope",
        include_str!("../../proto/andromeda/protocol/v1/envelope.proto"),
    ),
    (
        "payload",
        include_str!("../../proto/andromeda/protocol/v1/payload.proto"),
    ),
    (
        "completion",
        include_str!("../../proto/andromeda/protocol/v1/completion.proto"),
    ),
    (
        "error",
        include_str!("../../proto/andromeda/protocol/v1/error.proto"),
    ),
    (
        "invocation",
        include_str!("../../proto/andromeda/protocol/v1/invocation.proto"),
    ),
];
const CONTRACT_SCHEMAS: &[(&str, &str)] = &[
    (
        "contract",
        include_str!("../../proto/andromeda/contract/v1/contract.proto"),
    ),
    (
        "manifest",
        include_str!("../../proto/andromeda/contract/v1/manifest.proto"),
    ),
];
const PROTO_MANIFEST: &str = include_str!("../../Cargo.toml");
const QUIC_MANIFEST: &str = include_str!("../../../andromeda-quic/Cargo.toml");
const EXEC_MANIFEST: &str = include_str!("../../../andromeda-exec/Cargo.toml");

use andromeda_core::{
    AndromedaErrorKind, CatalogVersion, ColumnDescriptor, ContractHash, ProcedureId, ScalarType,
    TypeDescriptor,
};
use andromeda_proto::{
    CONTRACT_PACKAGE, DESCRIPTOR_SET_HASH_ALGORITHM, ManifestPolicyVersion,
    PROTOCOL_FRAME_ENVELOPE_TYPE, PROTOCOL_PACKAGE, ProcedureManifest, ProtocolLayout,
    RequiredPermission, ResultCardinality, ResultStreamDescriptor, RowCountRequirement,
    StructuredObjectHeader, StructuredObjectLayout, descriptor_set_bytes, descriptor_set_hash,
    frame_envelope_hash, generated, protocol_layout,
};
use prost::Message;
use prost_types::FileDescriptorSet;

#[test]
fn governed_proto_registry_matches_crate_local_tree() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let proto_root = manifest_dir.join("proto");
    let discovered = collect_proto_relative_paths(&proto_root);
    let governed = CRATE_LOCAL_PROTO_SCHEMAS
        .iter()
        .map(|schema| schema.relative_path)
        .collect::<BTreeSet<_>>();

    assert_eq!(
        discovered, governed,
        "schema governance must include every crate-local .proto file and no external sources"
    );

    for schema in CRATE_LOCAL_PROTO_SCHEMAS {
        let crate_local_source = fs::read_to_string(manifest_dir.join(schema.relative_path))
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", schema.relative_path));
        assert_eq!(
            schema.source, crate_local_source,
            "{} must be included from the crate-local proto tree",
            schema.relative_path
        );
    }
}

#[test]
fn repository_root_schemas_proto_tree_remains_retired() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repository_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("andromeda-proto must remain under <repo>/crates/andromeda-proto");
    let legacy_proto_root = repository_root.join("schemas").join("proto");
    let legacy_proto_files = collect_proto_files_under(&legacy_proto_root);

    assert!(
        legacy_proto_files.is_empty(),
        "schemas/proto is retired; crates/andromeda-proto/proto/andromeda/** is the sole \
         protobuf source authority. Remove stale legacy files or add an explicit deterministic \
         mirror drift contract before retaining a mirror. Legacy files discovered: {legacy_proto_files:?}"
    );
}

#[test]
fn governed_schemas_stay_message_only_without_service_rpc_grpc_or_tonic_identifiers() {
    for schema in CRATE_LOCAL_PROTO_SCHEMAS {
        let active_schema = active_schema_text(schema.source);
        for token in schema_identifier_tokens(&active_schema) {
            let lower = token.to_ascii_lowercase();
            assert!(
                lower != "service",
                "{} schema must not define protobuf services",
                schema.logical_name
            );
            assert!(
                lower != "rpc",
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
        }
    }

    assert!(
        schema_contains(PROTOCOL_SCHEMAS, "QUIC DATAGRAM is"),
        "protocol schema should preserve the telemetry-only DATAGRAM governance note"
    );
}

#[test]
fn protocol_result_surface_does_not_add_grpc_or_runtime_json_dependencies() {
    for (name, manifest) in [
        ("andromeda-proto", PROTO_MANIFEST),
        ("andromeda-quic", QUIC_MANIFEST),
        ("andromeda-exec", EXEC_MANIFEST),
    ] {
        let lower = manifest.to_ascii_lowercase();
        for forbidden in ["grpc", "tonic", "serde_json"] {
            assert!(
                !lower.contains(forbidden),
                "{name} manifest must not expose {forbidden} on protocol/result surface"
            );
        }
    }
}

#[test]
fn governed_schemas_declare_package_layout() {
    for schema in CRATE_LOCAL_PROTO_SCHEMAS {
        let declarations = package_declarations(schema.source);
        assert_eq!(
            declarations,
            vec![schema.expected_package.to_string()],
            "{} must declare only package {}",
            schema.relative_path,
            schema.expected_package
        );
    }
    assert!(schema_contains(
        PROTOCOL_SCHEMAS,
        "import \"andromeda/contract/v1/contract.proto\";"
    ));
    assert_eq!(PROTOCOL_PACKAGE, "andromeda.protocol.v1");
    assert_eq!(CONTRACT_PACKAGE, "andromeda.contract.v1");
    assert_eq!(
        PROTOCOL_FRAME_ENVELOPE_TYPE,
        "andromeda.protocol.v1.FrameEnvelope"
    );
}

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
    let protocol_version =
        generated::andromeda::protocol::v1::ProtocolVersion { major: 1, minor: 0 };
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
    };
    let invocation = generated::protocol::v1::InvocationRequest {
        correlation: Some(generated::protocol::v1::InvocationCorrelation {
            request_id: Some(2),
            session_id: Some(3),
            trace_id: Some("trace-1".to_string()),
            contract_hash: Some(vec![7; ContractHash::LEN]),
            catalog_version: Some(1),
            invocation_id: None,
        }),
        execute_request: Some(generated::protocol::v1::RpcExecuteRequest {
            procedure_name: "Inventory.ReserveStock".to_string(),
            expected_contract_hash: vec![7; ContractHash::LEN],
            expected_catalog_version: 1,
            surface_scope: "inventory".to_string(),
            arguments: Vec::new(),
            budget: None,
        }),
    };

    assert_eq!(envelope.payload_kind, 7);
    assert_eq!(
        manifest.protocol_layout.unwrap().protocol_package,
        PROTOCOL_PACKAGE
    );
    assert_eq!(
        generated::andromeda::protocol::v1::PayloadKind::RpcCompletion as i32,
        8
    );
    assert!(invocation.correlation.unwrap().request_id.is_some());
}

#[test]
fn descriptor_hashes_are_nonzero_stable_and_fit_protocol_layout() {
    // NOTE: the descriptor_set_hash and frame_envelope_hash are deterministic
    // functions of the generated FileDescriptorSet bytes. Any intentional
    // change to `crates/andromeda-proto/proto/andromeda/**` is a governance
    // event that *must* shift these digests. Rather than pinning a byte
    // snapshot inline (which would silently rot on every legitimate schema
    // edit), we assert the structural invariants the rest of the contract
    // surface depends on:
    //   * neither hash is zero,
    //   * descriptor and frame hashes are distinct (domain separation),
    //   * the hashes are stable across calls (deterministic), and
    //   * the protocol layout assembled from them validates.
    // Drift detection across releases is provided by the catalog / release
    // manifest, not by an inline byte snapshot.
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
fn governed_schemas_declare_enriched_message_contracts_and_reserved_ranges() {
    let declared_messages = declared_message_names();

    for message in [
        "FrameEnvelope",
        "ProtocolVersion",
        "RpcExecuteRequest",
        "RpcCompletion",
        "ErrorEnvelope",
        "RpcMetadata",
        "RpcBatch",
        "ResultCompletionPolicy",
        "InvocationCorrelation",
        "InvocationRequest",
        "InvocationResponse",
    ] {
        assert!(
            declared_messages.contains(message),
            "protocol schema must include {message}"
        );
    }
    for message in [
        "ProtocolLayout",
        "ProcedureManifest",
        "ResultStreamDescriptor",
        "StructuredObjectHeader",
        "RequiredPermission",
    ] {
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
        "optional BackpressureMetadata backpressure = 10;",
        "CompletionShape completion_shape = 1;",
        "bytes expected_contract_hash = 2;",
        "repeated Argument arguments = 5;",
        "RequestBudget budget = 6;",
        "bytes structured_payload = 4;",
        "bool terminal_batch = 6;",
    ] {
        assert!(
            schema_contains(PROTOCOL_SCHEMAS, field),
            "protocol schema missing enriched field: {field}"
        );
    }

    for field in [
        "bytes descriptor_set_hash = 1;",
        "bytes frame_envelope_hash = 2;",
        "bytes descriptor_hash = 3;",
        "optional uint64 max_payload_length = 9;",
        "bytes policy_version = 7;",
        "repeated RequiredPermission required_permissions = 8;",
        "optional uint64 row_count_max = 6;",
        "repeated ColumnDescriptor fields = 10;",
        "ResultStreamDescriptor.RowCountRequirement row_count_policy = 11;",
    ] {
        assert!(
            schema_contains(CONTRACT_SCHEMAS, field),
            "contract schema missing enriched field: {field}"
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

fn schema_contains(schemas: &[(&str, &str)], needle: &str) -> bool {
    schemas.iter().any(|(_, schema)| schema.contains(needle))
}

fn bytes_contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn stable_source_name_hash<'a>(
    names: impl IntoIterator<Item = &'a str>,
    domain: &'static [u8],
) -> ContractHash {
    let mut bytes = Vec::new();
    for name in names {
        let name_bytes = name.as_bytes();
        bytes.extend_from_slice(&(name_bytes.len() as u64).to_be_bytes());
        bytes.extend_from_slice(name_bytes);
    }

    ContractHash::new(stable_hash_256(domain, &bytes))
}

fn stable_hash_256(domain: &[u8], bytes: &[u8]) -> [u8; ContractHash::LEN] {
    let mut output = [0_u8; ContractHash::LEN];
    let seeds = [
        0xcbf2_9ce4_8422_2325_u64,
        0x8422_2325_cbf2_9ce4_u64,
        0x9e37_79b9_7f4a_7c15_u64,
        0x94d0_49bb_1331_11eb_u64,
    ];

    for (index, seed) in seeds.into_iter().enumerate() {
        let hash = stable_hash64(seed, domain, bytes);
        let start = index * 8;
        output[start..start + 8].copy_from_slice(&hash.to_be_bytes());
    }

    output
}

fn stable_hash64(seed: u64, domain: &[u8], bytes: &[u8]) -> u64 {
    let mut hash = seed ^ ((domain.len() as u64) << 32) ^ bytes.len() as u64;

    for byte in domain {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        hash ^= hash.rotate_left(17);
    }

    hash ^= 0xff;
    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);

    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        hash ^= hash.rotate_left(31);
    }

    hash
}

fn collect_proto_relative_paths(proto_root: &Path) -> BTreeSet<&'static str> {
    let mut paths = fs::read_dir(proto_root)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", proto_root.display()))
        .flat_map(|entry| {
            let entry = entry.unwrap_or_else(|error| panic!("failed to read proto entry: {error}"));
            let path = entry.path();
            if path.is_dir() {
                collect_proto_relative_paths_from(&path)
            } else {
                proto_relative_path_if_schema(&path).into_iter().collect()
            }
        })
        .collect::<Vec<_>>();
    paths.sort_unstable();
    paths.into_iter().collect()
}

fn collect_proto_relative_paths_from(directory: &Path) -> Vec<&'static str> {
    let mut collected = Vec::new();
    for entry in fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
    {
        let entry = entry.unwrap_or_else(|error| panic!("failed to read proto entry: {error}"));
        let path = entry.path();
        if path.is_dir() {
            collected.extend(collect_proto_relative_paths_from(&path));
        } else if let Some(relative_path) = proto_relative_path_if_schema(&path) {
            collected.push(relative_path);
        }
    }
    collected
}

fn proto_relative_path_if_schema(path: &Path) -> Option<&'static str> {
    let relative = path
        .strip_prefix(env!("CARGO_MANIFEST_DIR"))
        .ok()?
        .to_string_lossy()
        .replace('\\', "/");
    let relative = relative.strip_prefix('/').unwrap_or(&relative);
    CRATE_LOCAL_PROTO_SCHEMAS
        .iter()
        .find(|schema| schema.relative_path == relative)
        .map(|schema| schema.relative_path)
        .or_else(|| {
            if path.extension().and_then(|extension| extension.to_str()) == Some("proto") {
                panic!("ungoverned crate-local proto file discovered: {relative}");
            }
            None
        })
}

fn collect_proto_files_under(root: &Path) -> Vec<String> {
    if !root.exists() {
        return Vec::new();
    }

    let mut files = Vec::new();
    collect_proto_files_under_from(root, root, &mut files);
    files.sort();
    files
}

fn collect_proto_files_under_from(root: &Path, directory: &Path, files: &mut Vec<String>) {
    for entry in fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
    {
        let entry = entry.unwrap_or_else(|error| panic!("failed to read proto entry: {error}"));
        let path = entry.path();
        if path.is_dir() {
            collect_proto_files_under_from(root, &path, files);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("proto") {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            files.push(relative);
        }
    }
}

fn active_schema_text(schema: &str) -> String {
    let mut active = String::with_capacity(schema.len());
    let mut chars = schema.chars().peekable();
    let mut in_block_comment = false;
    let mut in_string = false;

    while let Some(character) = chars.next() {
        if in_block_comment {
            if character == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block_comment = false;
            }
            if character == '\n' {
                active.push('\n');
            }
            continue;
        }

        if in_string {
            if character == '\\' {
                chars.next();
                continue;
            }
            if character == '"' {
                in_string = false;
            }
            if character == '\n' {
                active.push('\n');
            }
            continue;
        }

        if character == '/' && chars.peek() == Some(&'/') {
            for comment_character in chars.by_ref() {
                if comment_character == '\n' {
                    active.push('\n');
                    break;
                }
            }
            continue;
        }

        if character == '/' && chars.peek() == Some(&'*') {
            chars.next();
            in_block_comment = true;
            continue;
        }

        if character == '"' {
            in_string = true;
            continue;
        }

        active.push(character);
    }

    active
}

fn schema_identifier_tokens(active_schema: &str) -> Vec<&str> {
    active_schema
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|token| !token.is_empty())
        .collect()
}

fn package_declarations(schema: &str) -> Vec<String> {
    active_schema_text(schema)
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix("package ")
                .and_then(|declaration| declaration.strip_suffix(';'))
                .map(str::trim)
                .map(str::to_string)
        })
        .collect()
}

fn declared_message_names() -> BTreeSet<String> {
    let mut names = BTreeSet::new();

    for schema in CRATE_LOCAL_PROTO_SCHEMAS {
        let active_schema = active_schema_text(schema.source);
        for line in active_schema.lines() {
            if let Some(message_name) = line
                .trim()
                .strip_prefix("message ")
                .and_then(|rest| rest.split_whitespace().next())
                .map(|name| name.trim_end_matches('{'))
            {
                names.insert(message_name.to_string());
            }
        }
    }

    names
}

enum SchemaBlock {
    Message {
        qualified_name: String,
        has_reserved_range: bool,
    },
    Other,
}

fn assert_all_message_definitions_have_reserved_ranges() {
    for schema in CRATE_LOCAL_PROTO_SCHEMAS {
        let active_schema = active_schema_text(schema.source);
        let mut stack = Vec::<SchemaBlock>::new();
        let mut message_path = Vec::<String>::new();

        for (line_index, line) in active_schema.lines().enumerate() {
            let trimmed = line.trim();

            if let Some(message_name) = trimmed
                .strip_prefix("message ")
                .and_then(|rest| rest.split_whitespace().next())
                .map(|name| name.trim_end_matches('{'))
            {
                message_path.push(message_name.to_string());
                stack.push(SchemaBlock::Message {
                    qualified_name: message_path.join("."),
                    has_reserved_range: false,
                });
            } else if trimmed.starts_with("enum ") || trimmed.starts_with("oneof ") {
                stack.push(SchemaBlock::Other);
            } else if trimmed.starts_with("reserved ") {
                if let Some(SchemaBlock::Message {
                    has_reserved_range, ..
                }) = stack.last_mut()
                {
                    *has_reserved_range = true;
                }
            }

            for _ in 0..trimmed
                .chars()
                .filter(|character| *character == '}')
                .count()
            {
                match stack.pop() {
                    Some(SchemaBlock::Message {
                        qualified_name,
                        has_reserved_range,
                    }) => {
                        assert!(
                            has_reserved_range,
                            "{}:{} message {qualified_name} must declare a reserved range",
                            schema.relative_path,
                            line_index + 1
                        );
                        message_path.pop();
                    }
                    Some(SchemaBlock::Other) => {}
                    None => panic!(
                        "{}:{} has an unmatched closing brace",
                        schema.relative_path,
                        line_index + 1
                    ),
                }
            }
        }

        assert!(
            stack.is_empty(),
            "{} must not leave message/enum/oneof blocks unclosed",
            schema.relative_path
        );
    }
}

#[test]
fn structured_object_header_validates_shape_hash_and_payload_bounds() {
    let layout = StructuredObjectLayout::RowMajor;
    let fields = vec![
        ColumnDescriptor {
            name: "ProductId".to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal: 0,
        },
        ColumnDescriptor {
            name: "Quantity".to_string(),
            data_type: TypeDescriptor::required(ScalarType::I32),
            ordinal: 1,
        },
    ];
    let descriptor_hash = StructuredObjectHeader::compute_descriptor_hash(&fields, layout);
    let valid = StructuredObjectHeader {
        name: "Reservation".to_string(),
        contract_hash: ContractHash::test_vector(7),
        descriptor_hash,
        column_count: fields.len() as u32,
        fields,
        layout,
        row_count_policy: RowCountRequirement::ExactRequired,
        row_count_exact: Some(1),
        payload_length: 128,
        payload_checksum: Some(0xA5A5),
        max_payload_length: Some(256),
    };

    assert!(valid.validate().is_ok());

    let zero_descriptor_hash = StructuredObjectHeader {
        descriptor_hash: ContractHash::zero(),
        ..valid.clone()
    };
    assert_eq!(
        zero_descriptor_hash.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );

    let over_bound = StructuredObjectHeader {
        payload_length: 257,
        ..valid
    };
    assert_eq!(
        over_bound.validate().unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn result_stream_descriptor_aligns_cardinality_and_exact_row_count_requirement() {
    let descriptor = ResultStreamDescriptor {
        stream_name: "Reservation".to_string(),
        columns: vec![ColumnDescriptor {
            name: "ProductId".to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal: 0,
        }],
        cardinality: ResultCardinality::ExactlyOne,
        row_count_requirement: RowCountRequirement::ExactRequired,
        row_count_exact: Some(1),
        row_count_max: Some(1),
    };

    assert!(descriptor.validate().is_ok());

    let missing_exact = ResultStreamDescriptor {
        row_count_exact: None,
        ..descriptor.clone()
    };
    assert_eq!(
        missing_exact.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );

    let wrong_cardinality = ResultStreamDescriptor {
        row_count_exact: Some(2),
        row_count_max: Some(2),
        ..descriptor
    };
    assert_eq!(
        wrong_cardinality.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );
}

fn governance_sample_manifest() -> ProcedureManifest {
    ProcedureManifest {
        procedure_id: ProcedureId::new(101),
        procedure_name: "Inventory.ReserveStock".to_string(),
        contract_hash: ContractHash::test_vector(0x11),
        catalog_version: CatalogVersion::new(3),
        policy_version: ManifestPolicyVersion::test_vector(0x22),
        protocol_layout: ProtocolLayout {
            descriptor_set_hash: descriptor_set_hash(),
            frame_envelope_hash: frame_envelope_hash(),
        },
        result_streams: vec![ResultStreamDescriptor {
            stream_name: "Reservation".to_string(),
            columns: vec![ColumnDescriptor {
                name: "ProductId".to_string(),
                data_type: TypeDescriptor::required(ScalarType::I64),
                ordinal: 0,
            }],
            cardinality: ResultCardinality::ExactlyOne,
            row_count_requirement: RowCountRequirement::ExactRequired,
            row_count_exact: Some(1),
            row_count_max: Some(1),
        }],
        required_permissions: vec![RequiredPermission::new(
            "andromeda.execute_procedure",
            "application",
        )],
    }
}

#[test]
fn procedure_manifest_hash_is_deterministic_and_distinct_from_descriptor_hashes() {
    let manifest = governance_sample_manifest();
    let hash = manifest.manifest_hash();

    // Determinism: identical inputs produce identical digests across calls.
    assert_eq!(hash, governance_sample_manifest().manifest_hash());

    // Manifest digest must not collide with descriptor / frame / contract hashes,
    // each of which has its own purpose in the contract surface.
    assert_ne!(hash, descriptor_set_hash());
    assert_ne!(hash, frame_envelope_hash());
    assert_ne!(hash, manifest.contract_hash);
    assert!(!hash.is_zero());

    // Field sensitivity: changing any participating field flips the digest.
    let mut bumped = governance_sample_manifest();
    bumped.policy_version = ManifestPolicyVersion::test_vector(0x99);
    assert_ne!(hash, bumped.manifest_hash());
}

#[test]
fn procedure_manifest_enforces_generator_readiness_and_permission_policy_presence() {
    let manifest = governance_sample_manifest();
    assert!(manifest.validate().is_ok());
    assert!(manifest.ensure_source_generator_ready().is_ok());

    // Missing policy version is a generator-blocker but passes plain validate.
    let mut zero_policy = governance_sample_manifest();
    zero_policy.policy_version = ManifestPolicyVersion::zero();
    assert!(zero_policy.validate().is_ok());
    assert_eq!(
        zero_policy
            .ensure_source_generator_ready()
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Contract
    );

    // Empty permissions are a generator-blocker.
    let mut no_perms = governance_sample_manifest();
    no_perms.required_permissions.clear();
    assert_eq!(
        no_perms.ensure_source_generator_ready().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );

    // Descriptor-set / frame envelope hash collision must be rejected.
    let mut collide = governance_sample_manifest();
    collide.protocol_layout.frame_envelope_hash = collide.protocol_layout.descriptor_set_hash;
    assert_eq!(
        collide.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );
}
