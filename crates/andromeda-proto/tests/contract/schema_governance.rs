const PROTOCOL_SCHEMA: &str =
    include_str!("../../../../schemas/proto/andromeda/protocol/v1/protocol.proto");
const CONTRACT_SCHEMA: &str =
    include_str!("../../../../schemas/proto/andromeda/contract/v1/contract.proto");
const PROTO_MANIFEST: &str = include_str!("../../Cargo.toml");
const QUIC_MANIFEST: &str = include_str!("../../../andromeda-quic/Cargo.toml");
const EXEC_MANIFEST: &str = include_str!("../../../andromeda-exec/Cargo.toml");

use andromeda_core::{
    AndromedaErrorKind, CatalogVersion, ColumnDescriptor, ContractHash, ProcedureId, ScalarType,
    TypeDescriptor,
};
use andromeda_proto::{
    descriptor_set_bytes, descriptor_set_hash, frame_envelope_hash, generated, protocol_layout,
    ManifestPolicyVersion, ProcedureManifest, ProtocolLayout, RequiredPermission,
    ResultCardinality, ResultStreamDescriptor, RowCountRequirement, StructuredObjectHeader,
    StructuredObjectLayout, CONTRACT_PACKAGE, PROTOCOL_FRAME_ENVELOPE_TYPE, PROTOCOL_PACKAGE,
};

#[test]
fn governed_schemas_stay_message_only_without_service_definitions() {
    for (name, schema) in [("protocol", PROTOCOL_SCHEMA), ("contract", CONTRACT_SCHEMA)] {
        for line in schema.lines() {
            let trimmed = line.trim_start();
            assert!(
                !trimmed.starts_with("service "),
                "{name} schema must not define protobuf services: {line}"
            );
            assert!(
                !trimmed.starts_with("rpc "),
                "{name} schema must not define RPC service methods: {line}"
            );
        }
    }

    assert!(
        PROTOCOL_SCHEMA.contains("QUIC DATAGRAM is"),
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
    assert!(PROTOCOL_SCHEMA.contains("package andromeda.protocol.v1;"));
    assert!(CONTRACT_SCHEMA.contains("package andromeda.contract.v1;"));
    assert!(PROTOCOL_SCHEMA.contains("import \"andromeda/contract/v1/contract.proto\";"));
    assert_eq!(PROTOCOL_PACKAGE, "andromeda.protocol.v1");
    assert_eq!(CONTRACT_PACKAGE, "andromeda.contract.v1");
    assert_eq!(
        PROTOCOL_FRAME_ENVELOPE_TYPE,
        "andromeda.protocol.v1.FrameEnvelope"
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

    assert_eq!(envelope.payload_kind, 7);
    assert_eq!(
        manifest.protocol_layout.unwrap().protocol_package,
        PROTOCOL_PACKAGE
    );
    assert_eq!(
        generated::andromeda::protocol::v1::PayloadKind::RpcCompletion as i32,
        8
    );
}

#[test]
fn descriptor_hashes_are_nonzero_stable_and_fit_protocol_layout() {
    // NOTE: the descriptor_set_hash and frame_envelope_hash are deterministic
    // functions of the generated FileDescriptorSet bytes. Any intentional
    // change to `schemas/proto/andromeda/contract/v1/contract.proto` or
    // `schemas/proto/andromeda/protocol/v1/protocol.proto` is a governance
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
    for message in [
        "message RpcExecuteRequest",
        "message RpcCompletion",
        "message ErrorEnvelope",
        "message RpcMetadata",
        "message RpcBatch",
        "message ResultCompletionPolicy",
    ] {
        assert!(
            PROTOCOL_SCHEMA.contains(message),
            "protocol schema must include {message}"
        );
    }
    for message in [
        "message ProtocolLayout",
        "message ProcedureManifest",
        "message ResultStreamDescriptor",
        "message StructuredObjectHeader",
    ] {
        assert!(
            CONTRACT_SCHEMA.contains(message),
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
            PROTOCOL_SCHEMA.contains(field),
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
            CONTRACT_SCHEMA.contains(field),
            "contract schema missing enriched field: {field}"
        );
    }

    for message in ["message RequiredPermission"] {
        assert!(
            CONTRACT_SCHEMA.contains(message),
            "contract schema must include {message}"
        );
    }

    for reservation in [
        "reserved 4 to 31;",
        "reserved 38 to 63;",
        "reserved 11 to 31;",
    ] {
        assert!(
            PROTOCOL_SCHEMA.contains(reservation),
            "protocol schema missing reserved range: {reservation}"
        );
    }

    for reservation in [
        "reserved 7 to 31;",
        "reserved 9 to 31;",
        "reserved 12 to 31;",
    ] {
        assert!(
            CONTRACT_SCHEMA.contains(reservation),
            "contract schema missing reserved range: {reservation}"
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
