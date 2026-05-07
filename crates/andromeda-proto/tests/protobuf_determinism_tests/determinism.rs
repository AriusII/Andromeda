use prost::Message;

use andromeda_proto::generated::andromeda::{
    contract::v1::StructuredObjectHeader,
    protocol::v1::{ErrorEnvelope, FrameEnvelope, ProtocolVersion, RpcBatch},
};

use crate::proto_wire_fixtures::{
    HASH_LEN, TEST_CONTRACT_HASH_BYTE, TEST_DESCRIPTOR_HASH_BYTE,
    generated_protocol_v1 as protocol_v1,
};

#[test]
fn test_protocol_version_deterministic_serialization() {
    let version = protocol_v1();

    let bytes1 = version.encode_to_vec();
    let bytes2 = version.encode_to_vec();

    assert_eq!(bytes1, bytes2, "Multiple serializations must be identical");
}

#[test]
fn test_protocol_version_roundtrip() {
    let original = ProtocolVersion { major: 1, minor: 5 };

    let bytes = original.encode_to_vec();
    let decoded =
        ProtocolVersion::decode(bytes.as_slice()).expect("Deserialization should succeed");

    assert_eq!(original, decoded, "Round-trip must preserve all fields");
}

#[test]
fn test_frame_envelope_deterministic_serialization() {
    let envelope = FrameEnvelope {
        protocol_version: Some(protocol_v1()),
        contract_hash: vec![TEST_CONTRACT_HASH_BYTE; HASH_LEN],
        catalog_version: 12345,
        request_id: 10,
        session_id: 20,
        tx_id: Some(30),
        payload_kind: 5,
        payload: vec![1, 2, 3, 4, 5],
    };

    let bytes1 = envelope.encode_to_vec();
    let bytes2 = envelope.encode_to_vec();
    let bytes3 = envelope.encode_to_vec();

    assert_eq!(bytes1, bytes2, "First and second serializations must match");
    assert_eq!(bytes2, bytes3, "Second and third serializations must match");
}

#[test]
fn test_frame_envelope_edge_cases() {
    let envelope_empty_hash = FrameEnvelope {
        protocol_version: Some(protocol_v1()),
        contract_hash: vec![],
        catalog_version: 0,
        request_id: 0,
        session_id: 0,
        tx_id: None,
        payload_kind: 1,
        payload: vec![],
    };

    let bytes = envelope_empty_hash.encode_to_vec();
    let decoded = FrameEnvelope::decode(bytes.as_slice()).expect("Deserialize with empty hash");

    assert_eq!(envelope_empty_hash, decoded);
}

#[test]
fn test_rpc_batch_deterministic_serialization() {
    let batch = RpcBatch {
        result_name: "users".to_string(),
        batch_index: 1,
        rows_emitted: 100,
        structured_payload: vec![0, 1, 2, 3, 255],
        row_count_exact: Some(1000),
        terminal_batch: false,
    };

    let bytes1 = batch.encode_to_vec();
    let bytes2 = batch.encode_to_vec();

    assert_eq!(bytes1, bytes2, "Batch serializations must be identical");
}

#[test]
fn test_error_envelope_deterministic_serialization() {
    let error = ErrorEnvelope {
        request_id: Some(10),
        session_id: Some(20),
        trace_id: Some("trace-001".to_string()),
        family: 1,
        code: "PROTOCOL_ERROR".to_string(),
        message: "Invalid frame type".to_string(),
        transaction_effect: 1,
        retry_disposition: 1,
        retry_after_ms: None,
        backpressure: None,
    };

    let bytes1 = error.encode_to_vec();
    let bytes2 = error.encode_to_vec();

    assert_eq!(bytes1, bytes2, "Error envelope serializations must match");
}

#[test]
fn test_structured_object_header_deterministic_serialization() {
    let header = StructuredObjectHeader {
        name: "result_set".to_string(),
        contract_hash: vec![TEST_CONTRACT_HASH_BYTE; HASH_LEN],
        descriptor_hash: vec![TEST_DESCRIPTOR_HASH_BYTE; HASH_LEN],
        row_count_exact: 1000,
        column_count: 5,
        layout: 1,
        payload_length: 50000,
        payload_checksum: Some(0xDEADBEEF),
        max_payload_length: Some(100000),
        fields: vec![],
        row_count_policy: 2,
    };

    let bytes1 = header.encode_to_vec();
    let bytes2 = header.encode_to_vec();

    assert_eq!(
        bytes1, bytes2,
        "Structured object headers must serialize identically"
    );
}

// Test: Verify known hash values for specific messages (contract binding)
#[test]
fn test_known_message_encoding_contracts() {
    // ProtocolVersion(1, 0) must always encode field 1 as varint 1. The
    // zero-valued minor field is omitted by Protobuf canonical encoding.
    let version_1_0 = protocol_v1();
    let expected_bytes = vec![0x08, 0x01];
    let actual_bytes = version_1_0.encode_to_vec();

    assert_eq!(
        actual_bytes, expected_bytes,
        "ProtocolVersion(1, 0) encoding contract changed; verify compatibility"
    );
}
