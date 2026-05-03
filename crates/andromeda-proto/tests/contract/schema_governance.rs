const PROTOCOL_SCHEMA: &str =
    include_str!("../../../../schemas/proto/andromeda/protocol/v1/protocol.proto");
const CONTRACT_SCHEMA: &str =
    include_str!("../../../../schemas/proto/andromeda/contract/v1/contract.proto");
const PROTO_MANIFEST: &str = include_str!("../../Cargo.toml");
const QUIC_MANIFEST: &str = include_str!("../../../andromeda-quic/Cargo.toml");
const EXEC_MANIFEST: &str = include_str!("../../../andromeda-exec/Cargo.toml");

use andromeda_core::{
    AndromedaErrorKind, ColumnDescriptor, ContractHash, ScalarType, TypeDescriptor,
};
use andromeda_proto::{
    descriptor_set_bytes, descriptor_set_hash, frame_envelope_hash, generated, protocol_layout,
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
    const EXPECTED_DESCRIPTOR_SET_HASH: [u8; ContractHash::LEN] = [
        0xf3, 0x65, 0xfc, 0x3b, 0x20, 0x24, 0x2e, 0x39, 0x4c, 0xb6, 0x88, 0x40, 0x1d, 0x2d, 0x4a,
        0x47, 0x85, 0x00, 0x75, 0xa9, 0x6d, 0x7d, 0xbb, 0x46, 0x3d, 0x48, 0x8e, 0xec, 0x33, 0x78,
        0xa3, 0x56,
    ];
    const EXPECTED_FRAME_ENVELOPE_HASH: [u8; ContractHash::LEN] = [
        0x4e, 0x76, 0xae, 0x7c, 0x3d, 0xb0, 0xd9, 0x1b, 0xf3, 0xfc, 0xe3, 0x67, 0x10, 0xca, 0xc4,
        0xe4, 0x9a, 0x26, 0xcc, 0x1e, 0x1c, 0xb9, 0x15, 0xdd, 0x7b, 0x01, 0x89, 0x48, 0xfe, 0xe9,
        0x07, 0xc4,
    ];

    let descriptor_hash = descriptor_set_hash();
    let frame_hash = frame_envelope_hash();
    let layout = protocol_layout();

    assert!(!descriptor_set_bytes().is_empty());
    assert!(!descriptor_hash.is_zero());
    assert!(!frame_hash.is_zero());
    assert_eq!(descriptor_hash.as_bytes(), EXPECTED_DESCRIPTOR_SET_HASH);
    assert_eq!(frame_hash.as_bytes(), EXPECTED_FRAME_ENVELOPE_HASH);
    assert_eq!(descriptor_hash, descriptor_set_hash());
    assert_eq!(frame_hash, frame_envelope_hash());
    assert_eq!(layout.descriptor_set_hash, descriptor_hash);
    assert_eq!(layout.frame_envelope_hash, frame_hash);
    assert!(layout.validate().is_ok());
}

#[test]
fn governed_schemas_declare_enriched_message_contracts_and_reserved_ranges() {
    for message in [
        "message RpcCompletion",
        "message ErrorEnvelope",
        "message RpcMetadata",
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
    ] {
        assert!(
            PROTOCOL_SCHEMA.contains(field),
            "protocol schema missing enriched field: {field}"
        );
    }

    for field in [
        "bytes descriptor_set_hash = 1;",
        "bytes frame_envelope_hash = 2;",
        "bytes shape_hash = 3;",
        "optional uint64 max_payload_length = 9;",
    ] {
        assert!(
            CONTRACT_SCHEMA.contains(field),
            "contract schema missing enriched field: {field}"
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

    for reservation in ["reserved 6 to 31;", "reserved 10 to 31;"] {
        assert!(
            CONTRACT_SCHEMA.contains(reservation),
            "contract schema missing reserved range: {reservation}"
        );
    }
}

#[test]
fn structured_object_header_validates_shape_hash_and_payload_bounds() {
    let valid = StructuredObjectHeader {
        name: "Reservation".to_string(),
        contract_hash: ContractHash::test_vector(7),
        shape_hash: ContractHash::test_vector(8),
        row_count_exact: 1,
        column_count: 2,
        layout: StructuredObjectLayout::RowMajor,
        payload_length: 128,
        payload_checksum: Some(0xA5A5),
        max_payload_length: Some(256),
    };

    assert!(valid.validate().is_ok());

    let zero_shape_hash = StructuredObjectHeader {
        shape_hash: ContractHash::zero(),
        ..valid.clone()
    };
    assert_eq!(
        zero_shape_hash.validate().unwrap_err().kind(),
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
        ..descriptor
    };
    assert_eq!(
        wrong_cardinality.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );
}
