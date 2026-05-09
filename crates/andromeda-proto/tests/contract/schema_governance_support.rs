use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use andromeda_procedure_contract::{
    ManifestPolicyVersion, ProcedureManifest, ProtocolLayout, RequiredPermission,
    ResultCardinality, ResultStreamDescriptor, RowCountRequirement,
};
use andromeda_proto::{descriptor_set_hash, frame_envelope_hash};
use andromeda_types::{
    CatalogVersion, ColumnDescriptor, ContractHash, ProcedureId, ScalarType, TypeDescriptor,
};
use prost_types::DescriptorProto;

pub(crate) struct ProtoSchema {
    pub(crate) logical_name: &'static str,
    pub(crate) relative_path: &'static str,
    pub(crate) expected_package: &'static str,
    pub(crate) source: &'static str,
}

pub(crate) const PROTOCOL_PACKAGE_DECLARATION: &str = "andromeda.protocol.v1";
pub(crate) const CONTRACT_PACKAGE_DECLARATION: &str = "andromeda.contract.v1";

pub(crate) const CRATE_LOCAL_PROTO_SCHEMAS: &[ProtoSchema] = &[
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
        logical_name: "protocol/wal",
        relative_path: "proto/andromeda/protocol/v1/wal.proto",
        expected_package: PROTOCOL_PACKAGE_DECLARATION,
        source: include_str!("../../proto/andromeda/protocol/v1/wal.proto"),
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
    ProtoSchema {
        logical_name: "contract/catalog",
        relative_path: "proto/andromeda/contract/v1/catalog.proto",
        expected_package: CONTRACT_PACKAGE_DECLARATION,
        source: include_str!("../../proto/andromeda/contract/v1/catalog.proto"),
    },
];

pub(crate) const PROTOCOL_SCHEMAS: &[(&str, &str)] = &[
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
    (
        "wal",
        include_str!("../../proto/andromeda/protocol/v1/wal.proto"),
    ),
];

pub(crate) const CONTRACT_SCHEMAS: &[(&str, &str)] = &[
    (
        "contract",
        include_str!("../../proto/andromeda/contract/v1/contract.proto"),
    ),
    (
        "manifest",
        include_str!("../../proto/andromeda/contract/v1/manifest.proto"),
    ),
    (
        "catalog",
        include_str!("../../proto/andromeda/contract/v1/catalog.proto"),
    ),
];

pub(crate) const PROTO_MANIFEST: &str = include_str!("../../Cargo.toml");
pub(crate) const QUIC_MANIFEST: &str = include_str!("../../../andromeda-quic/Cargo.toml");
pub(crate) const EXEC_MANIFEST: &str = include_str!("../../../andromeda-exec/Cargo.toml");
pub(crate) const BUILD_SCRIPT: &str = include_str!("../../build.rs");
pub(crate) const GENERATED_WRAPPER: &str = include_str!("../../src/generated.rs");
pub(crate) const RUNTIME_PROJECTION_FACADE_SOURCE: &str =
    include_str!("../../src/generated_validation/mod.rs");
pub(crate) const RUNTIME_PROJECTION_SOURCES: &[(&str, &str)] = &[
    (
        "proto/generated_validation/mod",
        RUNTIME_PROJECTION_FACADE_SOURCE,
    ),
    (
        "wire/generated_validation/mod",
        include_str!("../../../andromeda-proto-wire/src/generated_validation/mod.rs"),
    ),
    (
        "wire/generated_validation/common",
        include_str!("../../../andromeda-proto-wire/src/generated_validation/common.rs"),
    ),
    (
        "wire/generated_validation/error",
        include_str!("../../../andromeda-proto-wire/src/generated_validation/error.rs"),
    ),
    (
        "wire/generated_validation/frame_envelope",
        include_str!("../../../andromeda-proto-wire/src/generated_validation/frame_envelope.rs"),
    ),
    (
        "wire/generated_validation/invocation",
        include_str!("../../../andromeda-proto-wire/src/generated_validation/invocation.rs"),
    ),
    (
        "wire/generated_validation/result_stream",
        include_str!("../../../andromeda-proto-wire/src/generated_validation/result_stream.rs"),
    ),
    (
        "wire/generated_validation/rpc_result",
        include_str!("../../../andromeda-proto-wire/src/generated_validation/rpc_result.rs"),
    ),
    (
        "wire/generated_validation/structured_object",
        include_str!("../../../andromeda-proto-wire/src/generated_validation/structured_object.rs"),
    ),
    (
        "wire/generated_validation/catalog_manifest_resolution",
        include_str!(
            "../../../andromeda-proto-wire/src/generated_validation/catalog_manifest_resolution.rs"
        ),
    ),
];
pub(crate) const GENERATED_VALIDATION_MANIFEST_SOURCE: &str =
    include_str!("../../../andromeda-proto-wire/src/generated_validation/procedure_manifest.rs");

pub(crate) fn schema_contains(schemas: &[(&str, &str)], needle: &str) -> bool {
    schemas.iter().any(|(_, schema)| schema.contains(needle))
}

pub(crate) fn bytes_contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

pub(crate) fn stable_source_name_hash<'a>(
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

pub(crate) fn collect_proto_relative_paths(proto_root: &Path) -> BTreeSet<&'static str> {
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

pub(crate) fn collect_proto_files_under(root: &Path) -> Vec<String> {
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

pub(crate) fn active_schema_text(schema: &str) -> String {
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

pub(crate) fn schema_identifier_tokens(active_schema: &str) -> Vec<&str> {
    active_schema
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|token| !token.is_empty())
        .collect()
}

pub(crate) fn package_declarations(schema: &str) -> Vec<String> {
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

pub(crate) fn declared_message_names() -> BTreeSet<String> {
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

pub(crate) fn assert_descriptor_messages_have_no_deprecated_fields(
    file_name: &str,
    messages: &[DescriptorProto],
    path: &mut Vec<String>,
) {
    for message in messages {
        let message_name = message.name.as_deref().unwrap_or("<unnamed message>");
        path.push(message_name.to_string());
        for field in &message.field {
            let deprecated = field
                .options
                .as_ref()
                .and_then(|options| options.deprecated)
                .unwrap_or(false);
            let message_path = path.join(".");
            let field_name = field.name.as_deref().unwrap_or("<unnamed field>");
            assert!(
                !deprecated,
                "{file_name}:{message_path}.{field_name} is deprecated without a V1 migration-policy allow-list"
            );
        }

        assert_descriptor_messages_have_no_deprecated_fields(file_name, &message.nested_type, path);
        path.pop();
    }
}

enum SchemaBlock {
    Message {
        qualified_name: String,
        has_reserved_range: bool,
    },
    Other,
}

pub(crate) fn assert_all_message_definitions_have_reserved_ranges() {
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
            } else if trimmed.starts_with("reserved ")
                && let Some(SchemaBlock::Message {
                    has_reserved_range, ..
                }) = stack.last_mut()
            {
                *has_reserved_range = true;
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
                    },
                    Some(SchemaBlock::Other) => {},
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

pub(crate) fn governance_column_descriptor() -> ColumnDescriptor {
    ColumnDescriptor {
        name: "ProductId".to_string(),
        data_type: TypeDescriptor::required(ScalarType::I64),
        ordinal: 0,
    }
}

pub(crate) fn governance_result_stream_descriptor() -> ResultStreamDescriptor {
    ResultStreamDescriptor {
        stream_name: "Reservation".to_string(),
        columns: vec![governance_column_descriptor()],
        cardinality: ResultCardinality::ExactlyOne,
        row_count_requirement: RowCountRequirement::ExactRequired,
        row_count_exact: Some(1),
        row_count_max: Some(1),
    }
}

pub(crate) fn governance_sample_manifest() -> ProcedureManifest {
    ProcedureManifest {
        procedure_id: ProcedureId::new(101),
        procedure_name: "Inventory.ReserveStock".to_string(),
        contract_hash: ContractHash::test_vector(0x11),
        catalog_version: CatalogVersion::new(3),
        stats_version: 5,
        policy_version: ManifestPolicyVersion::test_vector(0x22),
        protocol_layout: ProtocolLayout {
            descriptor_set_hash: descriptor_set_hash(),
            frame_envelope_hash: frame_envelope_hash(),
        },
        result_streams: vec![governance_result_stream_descriptor()],
        required_permissions: vec![RequiredPermission::new(
            "andromeda.execute_procedure",
            "application",
        )],
    }
}

pub(crate) fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
