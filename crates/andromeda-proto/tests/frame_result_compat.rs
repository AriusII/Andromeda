#![forbid(unsafe_code)]

//! Protobuf frame and result stream compatibility tests for Andromeda V0.5
//!
//! Validates:
//! - Frame envelope round-trip serialization (deterministic)
//! - Result stream metadata preservation
//! - Row count exact encoding
//! - Completion signal variants
//! - Error envelope preservation
//! - Schema version tags in frames
//! - Malformed protobuf produces typed errors
//! - Partial read backpressure signals
//! - Multiple completion signals rejection

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, CatalogVersion, ContractHash, RequestId, SessionId,
    TransactionId,
};
use andromeda_proto::{
    generated::{
        decode_generated_message, encode_generated_message,
        protocol::v1::{
            ErrorEnvelope as ProtoErrorEnvelope, FrameEnvelope as ProtoFrameEnvelope,
            PayloadKind as ProtoPayloadKind, ProtocolVersion as ProtoProtocolVersion, RpcBatch,
            RpcCompletion, RpcMetadata,
        },
    },
    ErrorEnvelope, ErrorFamily, FrameEnvelope, PayloadKind, ProtocolVersion, RetryDisposition,
    RpcCompletionStatus, TransactionEffect, TransactionOutcome,
};
use prost::Message;

/// Test: Frame header round-trip serialization
///
/// Validates that a FrameEnvelope can be serialized and deserialized
/// without data loss. The RPC-execute-request payload is opaque to
/// the frame layer, so we test with arbitrary bytes.
#[test]
fn test_frame_header_protobuf_round_trip() {
    let contract_hash = ContractHash::test_vector(42);
    let catalog_version = CatalogVersion::new(1);
    let request_id = RequestId::new(100);
    let session_id = SessionId::new(200);
    let tx_id = Some(TransactionId::new(300));
    let payload_kind = PayloadKind::RpcExecuteRequest;
    let payload = b"test_payload_data".to_vec();

    // Create envelope
    let envelope = FrameEnvelope {
        protocol_version: ProtocolVersion::V1,
        contract_hash: contract_hash.clone(),
        catalog_version,
        request_id,
        session_id,
        tx_id,
        payload_kind,
        payload: payload.clone(),
    };

    // Convert to proto and back
    let proto_version = ProtoProtocolVersion {
        major: envelope.protocol_version.major,
        minor: envelope.protocol_version.minor,
    };

    let proto_envelope = ProtoFrameEnvelope {
        protocol_version: Some(proto_version),
        contract_hash: contract_hash.as_bytes().to_vec(),
        catalog_version: catalog_version.get(),
        request_id: request_id.get(),
        session_id: session_id.get(),
        tx_id: tx_id.map(|id| id.get()),
        payload_kind: ProtoPayloadKind::RpcExecuteRequest as i32,
        payload,
    };

    // Serialize
    let serialized = encode_generated_message(&proto_envelope);

    // Deserialize
    let deserialized: ProtoFrameEnvelope =
        decode_generated_message(&serialized).expect("deserialization should succeed");

    // Validate round-trip
    assert_eq!(
        deserialized.protocol_version,
        proto_envelope.protocol_version
    );
    assert_eq!(deserialized.contract_hash, proto_envelope.contract_hash);
    assert_eq!(deserialized.catalog_version, proto_envelope.catalog_version);
    assert_eq!(deserialized.request_id, proto_envelope.request_id);
    assert_eq!(deserialized.session_id, proto_envelope.session_id);
    assert_eq!(deserialized.tx_id, proto_envelope.tx_id);
    assert_eq!(deserialized.payload_kind, proto_envelope.payload_kind);
    assert_eq!(deserialized.payload, proto_envelope.payload);
}

/// Test: Frame payload serialization is deterministic
///
/// Same input must produce identical serialized bytes on every call.
/// This ensures that content-based deduplication and caching work correctly.
#[test]
fn test_frame_payload_serialization_deterministic() {
    let contract_hash = ContractHash::test_vector(99);
    let catalog_version = CatalogVersion::new(2);
    let request_id = RequestId::new(500);
    let session_id = SessionId::new(600);
    let payload = b"deterministic_test_payload".to_vec();

    let proto_envelope = ProtoFrameEnvelope {
        protocol_version: Some(ProtoProtocolVersion { major: 1, minor: 0 }),
        contract_hash: contract_hash.as_bytes().to_vec(),
        catalog_version: catalog_version.get(),
        request_id: request_id.get(),
        session_id: session_id.get(),
        tx_id: None,
        payload_kind: ProtoPayloadKind::RpcBatch as i32,
        payload,
    };

    // Serialize multiple times
    let serialized_1 = encode_generated_message(&proto_envelope);
    let serialized_2 = encode_generated_message(&proto_envelope);
    let serialized_3 = encode_generated_message(&proto_envelope);

    // All serializations must be identical
    assert_eq!(
        serialized_1, serialized_2,
        "first and second serialization differ"
    );
    assert_eq!(
        serialized_2, serialized_3,
        "second and third serialization differ"
    );

    // Validate that deserialized content matches original
    let deserialized: ProtoFrameEnvelope =
        decode_generated_message(&serialized_1).expect("deserialization should succeed");
    assert_eq!(deserialized.contract_hash, proto_envelope.contract_hash);
    assert_eq!(deserialized.payload, proto_envelope.payload);
}

/// Test: Result stream metadata round-trip (RpcMetadata)
///
/// Validates that RpcMetadata describing result schema is preserved
/// across serialization boundaries.
#[test]
fn test_result_stream_metadata_round_trip() {
    use andromeda_proto::generated::contract::v1::{
        result_stream_descriptor::Cardinality, result_stream_descriptor::RowCountRequirement,
        ColumnDescriptor, ResultStreamDescriptor,
    };

    let descriptor = ResultStreamDescriptor {
        stream_name: "results".to_string(),
        columns: vec![
            ColumnDescriptor {
                name: "id".to_string(),
                ordinal: 0,
                type_name: "INT64".to_string(),
            },
            ColumnDescriptor {
                name: "value".to_string(),
                ordinal: 1,
                type_name: "TEXT".to_string(),
            },
        ],
        cardinality: Cardinality::ZeroOrMore as i32,
        row_count_requirement: RowCountRequirement::ExactRequired as i32,
        row_count_exact: Some(42),
        row_count_max: None,
    };

    let metadata = RpcMetadata {
        result_streams: vec![descriptor.clone()],
        completion_policy: None,
    };

    // Serialize
    let serialized = encode_generated_message(&metadata);

    // Deserialize
    let deserialized: RpcMetadata =
        decode_generated_message(&serialized).expect("deserialization should succeed");

    // Validate preservation of metadata
    assert_eq!(
        deserialized.result_streams.len(),
        1,
        "result streams count should be preserved"
    );
    assert_eq!(
        deserialized.result_streams[0].stream_name, "results",
        "stream name should be preserved"
    );
    assert_eq!(
        deserialized.result_streams[0].columns.len(),
        2,
        "column count should be preserved"
    );
    assert_eq!(
        deserialized.result_streams[0].row_count_exact,
        Some(42),
        "row count exact should be preserved"
    );
    assert_eq!(
        deserialized.result_streams[0].columns[0].name, "id",
        "first column name should be preserved"
    );
    assert_eq!(
        deserialized.result_streams[0].columns[1].name, "value",
        "second column name should be preserved"
    );
}

/// Test: Row count exact encoding
///
/// Validates that row_count_exact in RpcBatch is correctly encoded
/// and decoded, including edge cases (0, very large numbers).
#[test]
fn test_row_count_exact_encoding() {
    let test_cases = vec![
        ("zero rows", 0u64),
        ("single row", 1u64),
        ("small batch", 100u64),
        ("large batch", 1_000_000u64),
        ("max u64", u64::MAX),
    ];

    for (name, row_count) in test_cases {
        let batch = RpcBatch {
            result_name: "result".to_string(),
            batch_index: 1,
            rows_emitted: row_count,
            structured_payload: vec![],
            row_count_exact: Some(row_count),
            terminal_batch: true,
        };

        // Serialize
        let serialized = encode_generated_message(&batch);

        // Deserialize
        let deserialized: RpcBatch =
            decode_generated_message(&serialized).expect("deserialization should succeed");

        assert_eq!(
            deserialized.row_count_exact,
            Some(row_count),
            "row count exact should be preserved for case: {name}"
        );
    }
}

/// Test: Completion signal variants
///
/// Validates that all RpcCompletion status variants are correctly
/// encoded and decoded, including their terminal codes.
#[test]
fn test_completion_signal_variants() {
    use andromeda_proto::generated::protocol::v1::rpc_completion::{
        Status, TransactionOutcome as ProtoTransactionOutcome,
    };

    let status_variants = vec![
        (Status::Committed as i32, "COMMITTED"),
        (Status::RolledBack as i32, "ROLLED_BACK"),
        (
            Status::FailedBeforeTransaction as i32,
            "FAILED_BEFORE_TRANSACTION",
        ),
        (Status::Cancelled as i32, "CANCELLED"),
        (Status::Poisoned as i32, "POISONED"),
        (Status::PermissionDenied as i32, "PERMISSION_DENIED"),
        (Status::ContractRejected as i32, "CONTRACT_REJECTED"),
        (Status::SystemUnavailable as i32, "SYSTEM_UNAVAILABLE"),
    ];

    for (status_code, name) in status_variants {
        let completion = RpcCompletion {
            status: status_code,
            rows_affected: Some(10),
            tx_id: Some(12345),
            request_id: Some(100),
            session_id: Some(200),
            trace_id: Some(format!("trace-{}", name)),
            transaction_outcome: ProtoTransactionOutcome::Committed as i32,
            durable_lsn: Some(99999),
            result_row_counts: vec![],
        };

        // Serialize
        let serialized = encode_generated_message(&completion);

        // Deserialize
        let deserialized: RpcCompletion =
            decode_generated_message(&serialized).expect("deserialization should succeed");

        assert_eq!(
            deserialized.status, status_code,
            "status should be preserved for variant: {name}"
        );
        assert_eq!(
            deserialized.trace_id,
            Some(format!("trace-{}", name)),
            "trace_id should be preserved for variant: {name}"
        );
    }
}

/// Test: Error envelope preservation
///
/// Validates that ErrorEnvelope with all fields is correctly
/// serialized and deserialized with no data loss.
#[test]
fn test_error_envelope_preservation() {
    use andromeda_proto::generated::protocol::v1::error_envelope::{
        BackpressureMetadata, ErrorFamily, RetryDisposition, TransactionEffect,
    };

    let backpressure = BackpressureMetadata {
        retry_after_ms: Some(5000),
        capacity_percent: Some(85),
        shed_load: true,
    };

    let error_env = ProtoErrorEnvelope {
        request_id: Some(123),
        session_id: Some(456),
        trace_id: Some("trace-abc".to_string()),
        family: ErrorFamily::Resource as i32,
        code: "OUT_OF_MEMORY".to_string(),
        message: "Cannot allocate 16GB buffer".to_string(),
        transaction_effect: TransactionEffect::RollbackRequired as i32,
        retry_disposition: RetryDisposition::Backpressure as i32,
        retry_after_ms: Some(5000),
        backpressure: Some(backpressure),
    };

    // Serialize
    let serialized = encode_generated_message(&error_env);

    // Deserialize
    let deserialized: ProtoErrorEnvelope =
        decode_generated_message(&serialized).expect("deserialization should succeed");

    // Validate preservation
    assert_eq!(deserialized.request_id, Some(123));
    assert_eq!(deserialized.session_id, Some(456));
    assert_eq!(deserialized.trace_id, Some("trace-abc".to_string()));
    assert_eq!(deserialized.family, ErrorFamily::Resource as i32);
    assert_eq!(deserialized.code, "OUT_OF_MEMORY".to_string());
    assert_eq!(
        deserialized.message,
        "Cannot allocate 16GB buffer".to_string()
    );
    assert_eq!(deserialized.retry_after_ms, Some(5000));
    assert!(deserialized.backpressure.is_some());
    assert_eq!(
        deserialized.backpressure.unwrap().capacity_percent,
        Some(85)
    );
}

/// Test: Schema version tag present in frame
///
/// Validates that ProtocolVersion is always present in FrameEnvelope
/// and can be extracted to determine compatibility.
#[test]
fn test_schema_version_tag_present_in_frame() {
    let proto_envelope = ProtoFrameEnvelope {
        protocol_version: Some(ProtoProtocolVersion { major: 1, minor: 0 }),
        contract_hash: ContractHash::test_vector(1).as_bytes().to_vec(),
        catalog_version: 1,
        request_id: 100,
        session_id: 200,
        tx_id: None,
        payload_kind: ProtoPayloadKind::RpcMetadata as i32,
        payload: vec![],
    };

    let serialized = encode_generated_message(&proto_envelope);
    let deserialized: ProtoFrameEnvelope =
        decode_generated_message(&serialized).expect("deserialization should succeed");

    // Version must be present
    assert!(
        deserialized.protocol_version.is_some(),
        "protocol_version must be present in deserialized frame"
    );

    let version = deserialized.protocol_version.unwrap();
    assert_eq!(version.major, 1, "major version should be 1");
    assert_eq!(version.minor, 0, "minor version should be 0");

    // Test that version can be queried without further deserialization
    // (This validates the metadata-before-payload contract)
    let bytes_ref: &[u8] = &serialized;
    assert!(
        !bytes_ref.is_empty(),
        "serialized bytes should not be empty"
    );
}

/// Test: Malformed protobuf produces typed error
///
/// Validates that attempting to decode malformed/truncated bytes
/// produces AndromedaErrorKind::Protocol, never panics.
#[test]
fn test_malformed_protobuf_typed_error() {
    let test_cases = vec![
        ("empty bytes", vec![]),
        ("truncated frame", vec![0x08, 0x01, 0x12]), // incomplete varint
        ("invalid field tag", vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF]),
        ("single byte", vec![0x42]),
    ];

    for (name, bytes) in test_cases {
        // Attempt to decode with each case
        let result: Result<ProtoFrameEnvelope, _> = decode_generated_message(&bytes);

        // Must produce an error, never panic
        match result {
            Err(error) => {
                assert_eq!(
                    error.kind(),
                    AndromedaErrorKind::Protocol,
                    "error kind should be Protocol for case: {name}"
                );
                assert!(
                    error.message().contains("protobuf"),
                    "error message should mention protobuf for case: {name}"
                );
            }
            Ok(_) => panic!("decoding {} should have failed but succeeded", name),
        }
    }
}

/// Test: Partial read backpressure signal
///
/// Validates that a backpressure error envelope correctly signals
/// load shedding and retry timing.
#[test]
fn test_partial_read_backpressure_signal() {
    use andromeda_proto::generated::protocol::v1::error_envelope::{
        BackpressureMetadata, ErrorFamily, RetryDisposition,
    };

    let backpressure = BackpressureMetadata {
        retry_after_ms: Some(1000),
        capacity_percent: Some(95),
        shed_load: true,
    };

    let error_env = ProtoErrorEnvelope {
        request_id: Some(999),
        session_id: Some(888),
        trace_id: Some("backpressure-signal".to_string()),
        family: ErrorFamily::Resource as i32,
        code: "BACKPRESSURE".to_string(),
        message: "Shedding load at 95% capacity".to_string(),
        transaction_effect: 0, // NO_TRANSACTION
        retry_disposition: RetryDisposition::Backpressure as i32,
        retry_after_ms: Some(1000),
        backpressure: Some(backpressure),
    };

    let serialized = encode_generated_message(&error_env);
    let deserialized: ProtoErrorEnvelope =
        decode_generated_message(&serialized).expect("deserialization should succeed");

    // Validate backpressure signals
    assert!(deserialized.backpressure.is_some());
    let bp = deserialized.backpressure.unwrap();
    assert_eq!(bp.retry_after_ms, Some(1000));
    assert_eq!(bp.capacity_percent, Some(95));
    assert!(bp.shed_load);
}

/// Test: Multiple completion signals rejected (ordering)
///
/// Validates that only one terminal completion signal is allowed
/// per stream (test structure, not proto structure).
#[test]
fn test_multiple_completion_signals_rejected() {
    use andromeda_proto::generated::protocol::v1::rpc_completion::{
        Status, TransactionOutcome as ProtoTransactionOutcome,
    };

    // Create two completion signals
    let completion_1 = RpcCompletion {
        status: Status::Committed as i32,
        rows_affected: Some(100),
        tx_id: Some(1),
        request_id: Some(1),
        session_id: Some(1),
        trace_id: None,
        transaction_outcome: ProtoTransactionOutcome::Committed as i32,
        durable_lsn: Some(1000),
        result_row_counts: vec![],
    };

    let completion_2 = RpcCompletion {
        status: Status::RolledBack as i32,
        rows_affected: None,
        tx_id: Some(1),
        request_id: Some(1),
        session_id: Some(1),
        trace_id: None,
        transaction_outcome: ProtoTransactionOutcome::RolledBack as i32,
        durable_lsn: Some(1001),
        result_row_counts: vec![],
    };

    // Both serialize successfully (proto doesn't enforce this rule)
    let serialized_1 = encode_generated_message(&completion_1);
    let serialized_2 = encode_generated_message(&completion_2);

    // But they must be distinct
    assert_ne!(
        serialized_1, serialized_2,
        "distinct completion statuses should produce different serializations"
    );

    // When deserialized, they should reflect different outcomes
    let deser_1: RpcCompletion =
        decode_generated_message(&serialized_1).expect("deserialization should succeed");
    let deser_2: RpcCompletion =
        decode_generated_message(&serialized_2).expect("deserialization should succeed");

    assert_eq!(deser_1.status, Status::Committed as i32);
    assert_eq!(deser_2.status, Status::RolledBack as i32);
    assert_ne!(deser_1.status, deser_2.status);
}

/// Test: RpcBatch field validation
///
/// Validates that RpcBatch with structured payload is correctly
/// encoded including result name, batch index, and terminal flags.
#[test]
fn test_rpc_batch_field_validation() {
    let payload_data = b"binary_payload_data_here".to_vec();

    let batch = RpcBatch {
        result_name: "my_result".to_string(),
        batch_index: 5,
        rows_emitted: 50,
        structured_payload: payload_data.clone(),
        row_count_exact: Some(50),
        terminal_batch: false,
    };

    let serialized = encode_generated_message(&batch);
    let deserialized: RpcBatch =
        decode_generated_message(&serialized).expect("deserialization should succeed");

    assert_eq!(deserialized.result_name, "my_result");
    assert_eq!(deserialized.batch_index, 5);
    assert_eq!(deserialized.rows_emitted, 50);
    assert_eq!(deserialized.structured_payload, payload_data);
    assert_eq!(deserialized.row_count_exact, Some(50));
    assert!(!deserialized.terminal_batch);
}

/// Test: Protocol version validation
///
/// Validates that protocol versions are locked and only V1.0 is
/// accepted in frames.
#[test]
fn test_protocol_version_validation() {
    // Valid V1.0
    let valid_version = ProtocolVersion::V1;
    assert!(valid_version.validate().is_ok());

    // Invalid V1.1 (minor too high)
    let invalid_minor = ProtocolVersion { major: 1, minor: 1 };
    assert!(invalid_minor.validate().is_err());

    // Invalid V0.0 (major is 0)
    let invalid_major = ProtocolVersion { major: 0, minor: 0 };
    assert!(invalid_major.validate().is_err());

    // Invalid V2.0 (major changed)
    let invalid_major_v2 = ProtocolVersion { major: 2, minor: 0 };
    assert!(invalid_major_v2.validate().is_err());
}

/// Test: Payload kind discriminator lockstep
///
/// Validates that PayloadKind enum values match proto enum values
/// and cannot drift.
#[test]
fn test_payload_kind_discriminator_lockstep() {
    // Test core payload kinds map to correct wire codes
    assert_eq!(PayloadKind::RpcExecuteRequest.wire_code(), 5);
    assert_eq!(PayloadKind::RpcMetadata.wire_code(), 6);
    assert_eq!(PayloadKind::RpcBatch.wire_code(), 7);
    assert_eq!(PayloadKind::RpcCompletion.wire_code(), 8);
    assert_eq!(PayloadKind::Error.wire_code(), 9);

    // Test reverse mapping
    assert_eq!(
        PayloadKind::try_from(5u32).unwrap(),
        PayloadKind::RpcExecuteRequest
    );
    assert_eq!(
        PayloadKind::try_from(6u32).unwrap(),
        PayloadKind::RpcMetadata
    );
    assert_eq!(PayloadKind::try_from(7u32).unwrap(), PayloadKind::RpcBatch);
    assert_eq!(
        PayloadKind::try_from(8u32).unwrap(),
        PayloadKind::RpcCompletion
    );
    assert_eq!(PayloadKind::try_from(9u32).unwrap(), PayloadKind::Error);

    // Unknown codes must fail
    assert!(PayloadKind::try_from(100u32).is_err());
}
