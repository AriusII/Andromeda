const WIRE_SCHEMA: &str =
    include_str!("../../../../schemas/proto/andromeda/v1/andromeda_wire.proto");

use andromeda_core::{
    AndromedaErrorKind, ColumnDescriptor, ContractHash, ScalarType, TypeDescriptor,
};
use andromeda_proto::{
    ResultCardinality, ResultStreamDescriptor, RowCountRequirement, StructuredObjectHeader,
    StructuredObjectLayout,
};

#[test]
fn schema_sketch_stays_message_only_without_service_definitions() {
    for line in WIRE_SCHEMA.lines() {
        let trimmed = line.trim_start();
        assert!(
            !trimmed.starts_with("service "),
            "schema sketch must not define protobuf services: {line}"
        );
        assert!(
            !trimmed.starts_with("rpc "),
            "schema sketch must not define RPC service methods: {line}"
        );
    }

    assert!(
        WIRE_SCHEMA.contains("QUIC DATAGRAM is"),
        "schema sketch should preserve the telemetry-only DATAGRAM governance note"
    );
}

#[test]
fn schema_sketch_declares_enriched_message_contracts_and_reserved_ranges() {
    for message in [
        "message RpcCompletion",
        "message ErrorEnvelope",
        "message ResultStreamDescriptor",
        "message StructuredObjectHeader",
    ] {
        assert!(
            WIRE_SCHEMA.contains(message),
            "schema sketch must include {message}"
        );
    }

    for field in [
        "optional uint64 request_id = 32;",
        "optional uint64 session_id = 33;",
        "optional string trace_id = 34;",
        "optional uint64 durable_lsn = 36;",
        "repeated ResultRowCountSummary result_row_counts = 37;",
        "optional BackpressureMetadata backpressure = 10;",
        "bytes shape_hash = 3;",
        "optional uint64 max_payload_length = 9;",
    ] {
        assert!(
            WIRE_SCHEMA.contains(field),
            "schema sketch missing enriched field: {field}"
        );
    }

    for reservation in [
        "reserved 4 to 31;",
        "reserved 38 to 63;",
        "reserved 11 to 31;",
        "reserved 6 to 31;",
    ] {
        assert!(
            WIRE_SCHEMA.contains(reservation),
            "schema sketch missing reserved range: {reservation}"
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
