use andromeda_error::AndromedaErrorKind;
use andromeda_proto::generated::{
    decode_generated_message,
    protocol::v1::{
        ErrorEnvelope as ProtoErrorEnvelope, FrameEnvelope as ProtoFrameEnvelope,
        error_envelope::{BackpressureMetadata, ErrorFamily, RetryDisposition, TransactionEffect},
    },
};

use crate::proto_wire_fixtures::round_trip_generated;

/// Test: Error envelope preservation
///
/// Validates that ErrorEnvelope with all fields is correctly serialized and
/// deserialized with no data loss.
#[test]
fn test_error_envelope_preservation() {
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

    let deserialized = round_trip_generated(&error_env);

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

/// Test: Malformed protobuf produces typed error
///
/// Validates that attempting to decode malformed/truncated bytes produces
/// AndromedaErrorKind::Protocol, never panics.
#[test]
fn test_malformed_protobuf_typed_error() {
    let test_cases = [
        ("empty bytes", vec![]),
        ("truncated frame", vec![0x08, 0x01, 0x12]),
        ("invalid field tag", vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF]),
        ("single byte", vec![0x42]),
    ];

    for (name, bytes) in test_cases {
        let result: Result<ProtoFrameEnvelope, _> = decode_generated_message(&bytes);

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
            },
            Ok(_) => panic!("decoding {name} should have failed but succeeded"),
        }
    }
}

/// Test: Partial read backpressure signal
///
/// Validates that a backpressure error envelope correctly signals load
/// shedding and retry timing.
#[test]
fn test_partial_read_backpressure_signal() {
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
        transaction_effect: 0,
        retry_disposition: RetryDisposition::Backpressure as i32,
        retry_after_ms: Some(1000),
        backpressure: Some(backpressure),
    };

    let deserialized = round_trip_generated(&error_env);

    assert!(deserialized.backpressure.is_some());
    let bp = deserialized.backpressure.unwrap();
    assert_eq!(bp.retry_after_ms, Some(1000));
    assert_eq!(bp.capacity_percent, Some(95));
    assert!(bp.shed_load);
}
