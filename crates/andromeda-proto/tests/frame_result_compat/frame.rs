use andromeda_proto::generated::{
    decode_generated_message, encode_generated_message,
    protocol::v1::{FrameEnvelope as ProtoFrameEnvelope, PayloadKind as ProtoPayloadKind},
};
use andromeda_proto_wire::{FrameEnvelope, PayloadKind, ProtocolVersion};
use andromeda_types::{CatalogVersion, ContractHash, RequestId, SessionId, TransactionId};

use crate::proto_wire_fixtures::{
    generated_protocol_v1, proto_frame_envelope, round_trip_generated,
};

/// Test: Frame header round-trip serialization
///
/// Validates that a FrameEnvelope can be serialized and deserialized without
/// data loss. The RPC-execute-request payload is opaque to the frame layer, so
/// we test with arbitrary bytes.
#[test]
fn test_frame_header_protobuf_round_trip() {
    let contract_hash = ContractHash::test_vector(42);
    let catalog_version = CatalogVersion::new(1);
    let request_id = RequestId::new(100);
    let session_id = SessionId::new(200);
    let tx_id = Some(TransactionId::new(300));
    let payload_kind = PayloadKind::RpcExecuteRequest;
    let payload = b"test_payload_data".to_vec();

    let envelope = FrameEnvelope {
        protocol_version: ProtocolVersion::V1,
        contract_hash,
        catalog_version,
        request_id,
        session_id,
        tx_id,
        payload_kind,
        payload: payload.clone(),
    };

    let proto_envelope = proto_frame_envelope(
        contract_hash,
        catalog_version,
        request_id,
        session_id,
        tx_id,
        ProtoPayloadKind::RpcExecuteRequest,
        payload,
    );
    let deserialized = round_trip_generated(&proto_envelope);

    let proto_version = generated_protocol_v1();
    assert_eq!(envelope.protocol_version.major, proto_version.major);
    assert_eq!(envelope.protocol_version.minor, proto_version.minor);
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
/// Same input must produce identical serialized bytes on every call. This
/// ensures that content-based deduplication and caching work correctly.
#[test]
fn test_frame_payload_serialization_deterministic() {
    let contract_hash = ContractHash::test_vector(99);
    let catalog_version = CatalogVersion::new(2);
    let request_id = RequestId::new(500);
    let session_id = SessionId::new(600);
    let payload = b"deterministic_test_payload".to_vec();

    let proto_envelope = proto_frame_envelope(
        contract_hash,
        catalog_version,
        request_id,
        session_id,
        None,
        ProtoPayloadKind::RpcBatch,
        payload,
    );

    let serialized_1 = encode_generated_message(&proto_envelope);
    let serialized_2 = encode_generated_message(&proto_envelope);
    let serialized_3 = encode_generated_message(&proto_envelope);

    assert_eq!(
        serialized_1, serialized_2,
        "first and second serialization differ"
    );
    assert_eq!(
        serialized_2, serialized_3,
        "second and third serialization differ"
    );

    let deserialized: ProtoFrameEnvelope =
        decode_generated_message(&serialized_1).expect("deserialization should succeed");
    assert_eq!(deserialized.contract_hash, proto_envelope.contract_hash);
    assert_eq!(deserialized.payload, proto_envelope.payload);
}

/// Test: Schema version tag present in frame
///
/// Validates that ProtocolVersion is always present in FrameEnvelope and can be
/// extracted to determine compatibility.
#[test]
fn test_schema_version_tag_present_in_frame() {
    let proto_envelope = proto_frame_envelope(
        ContractHash::test_vector(1),
        CatalogVersion::new(1),
        RequestId::new(100),
        SessionId::new(200),
        None,
        ProtoPayloadKind::RpcMetadata,
        vec![],
    );

    let serialized = encode_generated_message(&proto_envelope);
    let deserialized: ProtoFrameEnvelope =
        decode_generated_message(&serialized).expect("deserialization should succeed");

    assert!(
        deserialized.protocol_version.is_some(),
        "protocol_version must be present in deserialized frame"
    );

    let version = deserialized.protocol_version.unwrap();
    assert_eq!(version.major, 1, "major version should be 1");
    assert_eq!(version.minor, 0, "minor version should be 0");

    let bytes_ref: &[u8] = &serialized;
    assert!(
        !bytes_ref.is_empty(),
        "serialized bytes should not be empty"
    );
}

/// Test: Protocol version validation
///
/// Validates that protocol versions are locked and only V1.0 is accepted in
/// frames.
#[test]
fn test_protocol_version_validation() {
    let valid_version = ProtocolVersion::V1;
    assert!(valid_version.validate().is_ok());

    let invalid_minor = ProtocolVersion { major: 1, minor: 1 };
    assert!(invalid_minor.validate().is_err());

    let invalid_major = ProtocolVersion { major: 0, minor: 0 };
    assert!(invalid_major.validate().is_err());

    let invalid_major_v2 = ProtocolVersion { major: 2, minor: 0 };
    assert!(invalid_major_v2.validate().is_err());
}

/// Test: Payload kind discriminator lockstep
///
/// Validates that PayloadKind enum values match proto enum values and cannot
/// drift.
#[test]
fn test_payload_kind_discriminator_lockstep() {
    assert_eq!(PayloadKind::RpcExecuteRequest.wire_code(), 5);
    assert_eq!(PayloadKind::RpcMetadata.wire_code(), 6);
    assert_eq!(PayloadKind::RpcBatch.wire_code(), 7);
    assert_eq!(PayloadKind::RpcCompletion.wire_code(), 8);
    assert_eq!(PayloadKind::Error.wire_code(), 9);

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
    assert!(PayloadKind::try_from(100u32).is_err());
}
