use andromeda_error::AndromedaErrorKind;
use andromeda_proto::{FrameEnvelope, PayloadKind, generated};
use andromeda_types::{CatalogVersion, ContractHash, RequestId, SessionId};

use super::fixtures::{envelope, hash};

const ENVELOPE_SCHEMA: &str = include_str!("../../../proto/andromeda/protocol/v1/envelope.proto");

#[test]
fn contract_bound_envelopes_require_nonzero_contract_hash() {
    for kind in [
        PayloadKind::RpcExecuteRequest,
        PayloadKind::RpcMetadata,
        PayloadKind::RpcBatch,
        PayloadKind::RpcCompletion,
    ] {
        assert!(kind.requires_contract_hash());

        let payload = if kind.requires_non_empty_payload() {
            b"payload".to_vec()
        } else {
            Vec::new()
        };
        let invalid = FrameEnvelope {
            contract_hash: ContractHash::zero(),
            payload_kind: kind,
            payload,
            ..envelope(kind, Vec::new())
        };

        assert_eq!(
            invalid.validate().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );
    }

    for kind in [
        PayloadKind::Hello,
        PayloadKind::Auth,
        PayloadKind::ContractRequest,
        PayloadKind::ContractResponse,
    ] {
        let contractless = FrameEnvelope {
            contract_hash: ContractHash::zero(),
            payload_kind: kind,
            payload: Vec::new(),
            ..envelope(kind, Vec::new())
        };

        assert!(!kind.requires_contract_hash());
        assert!(
            contractless.validate().is_ok(),
            "{kind:?} should not require a contract hash"
        );
    }

    let typed_error = FrameEnvelope {
        contract_hash: ContractHash::zero(),
        payload_kind: PayloadKind::Error,
        payload: b"typed-error-envelope".to_vec(),
        ..envelope(PayloadKind::Error, Vec::new())
    };
    assert!(!PayloadKind::Error.requires_contract_hash());
    assert!(typed_error.validate().is_ok());

    let empty_error = FrameEnvelope {
        payload: Vec::new(),
        ..typed_error
    };
    assert_eq!(
        empty_error.validate().unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn rpc_execute_and_batch_payload_bodies_are_required() {
    let execute = FrameEnvelope::rpc_execute_request(
        hash(9),
        CatalogVersion::new(12),
        RequestId::new(404),
        SessionId::new(505),
        None,
        Vec::new(),
    );
    assert_eq!(execute.unwrap_err().kind(), AndromedaErrorKind::Protocol);

    let batch = envelope(PayloadKind::RpcBatch, Vec::new());
    assert_eq!(
        batch.validate().unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );

    assert!(
        envelope(PayloadKind::RpcMetadata, Vec::new())
            .validate()
            .is_ok()
    );
    assert!(
        envelope(PayloadKind::RpcCompletion, Vec::new())
            .validate()
            .is_ok()
    );
}

#[test]
fn rpc_result_stream_envelopes_require_metadata_before_payload() {
    let metadata = envelope(PayloadKind::RpcMetadata, Vec::new());
    let batch = envelope(PayloadKind::RpcBatch, b"row-batch".to_vec());
    let completion = envelope(PayloadKind::RpcCompletion, Vec::new());

    FrameEnvelope::validate_rpc_stream_sequence(&[
        metadata.clone(),
        batch.clone(),
        completion.clone(),
    ])
    .unwrap();

    let error = FrameEnvelope::validate_rpc_stream_sequence(&[batch, metadata, completion])
        .expect_err("RPC batch payload must not be accepted before stream metadata");

    assert_eq!(error.kind(), AndromedaErrorKind::Protocol);
    assert!(
        error
            .message()
            .contains("metadata must precede RPC batch payloads"),
        "envelope sequence rejection should identify metadata-before-payload ordering"
    );
}

#[test]
fn generated_frame_envelope_rejects_unbound_or_unknown_payload_metadata() {
    let valid = generated::protocol::v1::FrameEnvelope {
        protocol_version: Some(generated::protocol::v1::ProtocolVersion { major: 1, minor: 0 }),
        contract_hash: vec![7; ContractHash::LEN],
        catalog_version: 11,
        request_id: 101,
        session_id: 202,
        tx_id: Some(303),
        payload_kind: generated::protocol::v1::PayloadKind::RpcBatch as i32,
        payload: b"row-batch".to_vec(),
    };

    generated::validate_generated_frame_envelope(&valid).unwrap();

    let missing_version = generated::protocol::v1::FrameEnvelope {
        protocol_version: None,
        ..valid.clone()
    };
    assert_eq!(
        generated::validate_generated_frame_envelope(&missing_version)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );

    let zero_catalog_version = generated::protocol::v1::FrameEnvelope {
        catalog_version: 0,
        ..valid.clone()
    };
    assert_eq!(
        generated::validate_generated_frame_envelope(&zero_catalog_version)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Contract
    );

    let unknown_payload_kind = generated::protocol::v1::FrameEnvelope {
        payload_kind: 200,
        ..valid
    };
    assert_eq!(
        generated::validate_generated_frame_envelope(&unknown_payload_kind)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn frame_envelope_schema_keeps_metadata_fields_before_payload_bytes() {
    let request_id = ENVELOPE_SCHEMA
        .find("uint64 request_id = 4;")
        .expect("FrameEnvelope must declare request_id metadata field 4");
    let session_id = ENVELOPE_SCHEMA
        .find("uint64 session_id = 5;")
        .expect("FrameEnvelope must declare session_id metadata field 5");
    let tx_id = ENVELOPE_SCHEMA
        .find("optional uint64 tx_id = 6;")
        .expect("FrameEnvelope must declare transaction metadata field 6");
    let payload_kind = ENVELOPE_SCHEMA
        .find("PayloadKind payload_kind = 7;")
        .expect("FrameEnvelope must declare payload kind field 7");
    let payload = ENVELOPE_SCHEMA
        .find("bytes payload = 8;")
        .expect("FrameEnvelope must declare opaque payload field 8");

    assert!(request_id < payload);
    assert!(session_id < payload);
    assert!(tx_id < payload);
    assert!(payload_kind < payload);
}
