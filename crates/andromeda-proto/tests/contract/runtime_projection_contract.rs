use andromeda_core::{
    AndromedaErrorKind, ColumnDescriptor, ContractHash, ScalarType, TypeDescriptor,
};
use andromeda_proto::{RowCountRequirement, StructuredObjectHeader, StructuredObjectLayout};

use super::support::{
    BUILD_SCRIPT, CONTRACT_SCHEMAS, EXEC_MANIFEST, GENERATED_VALIDATION_MANIFEST_SOURCE,
    PROTO_MANIFEST, QUIC_MANIFEST, RUNTIME_PROJECTION_SOURCES, active_schema_text, schema_contains,
    schema_identifier_tokens,
};

#[test]
fn protocol_result_surface_does_not_add_sql_grpc_or_runtime_json_dependencies() {
    for (name, manifest) in [
        ("andromeda-proto", PROTO_MANIFEST),
        ("andromeda-quic", QUIC_MANIFEST),
        ("andromeda-exec", EXEC_MANIFEST),
    ] {
        let lower = manifest.to_ascii_lowercase();
        for forbidden in [
            "grpc",
            "tonic",
            "json",
            "serde",
            "pbjson",
            "simd-json",
            "sql",
            "sqlx",
            "rusqlite",
            "diesel",
        ] {
            assert!(
                !lower.contains(forbidden),
                "{name} manifest must not expose {forbidden} on protocol/result surface"
            );
        }
    }
}

#[test]
fn protocol_runtime_validation_sources_do_not_add_sql_grpc_or_json_paths() {
    let sources = RUNTIME_PROJECTION_SOURCES
        .iter()
        .copied()
        .chain([(
            "generated_validation_manifest",
            GENERATED_VALIDATION_MANIFEST_SOURCE,
        )])
        .chain([("build_script", BUILD_SCRIPT)]);

    for (name, source) in sources {
        let active_source = active_schema_text(source);
        for token in schema_identifier_tokens(&active_source) {
            let lower = token.to_ascii_lowercase();
            for forbidden in [
                "grpc",
                "tonic",
                "json",
                "serde",
                "pbjson",
                "simd_json",
                "sql",
                "sqlx",
                "rusqlite",
                "diesel",
            ] {
                assert!(
                    !lower.contains(forbidden),
                    "{name} runtime validation source must not contain active {forbidden} identifiers"
                );
            }
        }
    }
}

#[test]
fn runtime_projection_schema_declares_structured_payload_bounds() {
    assert!(
        super::support::declared_message_names().contains("StructuredObjectHeader"),
        "contract schema must include StructuredObjectHeader"
    );

    for field in [
        "bytes descriptor_hash = 3;",
        "optional uint64 max_payload_length = 9;",
        "repeated ColumnDescriptor fields = 10;",
        "ResultStreamDescriptor.RowCountRequirement row_count_policy = 11;",
        "reserved \"shape_hash\";",
    ] {
        assert!(
            schema_contains(CONTRACT_SCHEMAS, field),
            "contract schema missing StructuredObject payload-bound field: {field}"
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
