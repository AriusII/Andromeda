use andromeda_error::AndromedaErrorKind;
use andromeda_proto::{FrameEnvelope, PayloadKind, RpcResultStreamMetadataPolicy};
use andromeda_proto_wire::{
    FrameEnvelope as WireFrameEnvelope, PayloadKind as WirePayloadKind,
    ProtocolVersion as WireProtocolVersion,
    RpcResultStreamMetadataPolicy as WireRpcResultStreamMetadataPolicy,
};
use andromeda_types::SessionId;
use andromeda_types::{CatalogVersion, ContractHash, RequestId, TransactionId};

use super::fixtures::envelope;

#[test]
fn envelope_sequence_requires_metadata_batch_completion_in_one_context() {
    let metadata = envelope(PayloadKind::RpcMetadata, b"columns".to_vec());
    let batch = envelope(PayloadKind::RpcBatch, b"row".to_vec());
    let completion = envelope(PayloadKind::RpcCompletion, Vec::new());

    assert!(
        FrameEnvelope::validate_rpc_stream_sequence(&[
            metadata.clone(),
            batch.clone(),
            completion.clone(),
        ])
        .is_ok()
    );

    assert_eq!(
        FrameEnvelope::validate_rpc_stream_sequence(&[metadata.clone(), completion.clone()])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
    assert_eq!(
        FrameEnvelope::validate_rpc_stream_sequence(&[batch.clone(), metadata.clone()])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );

    let mut wrong_context = batch.clone();
    wrong_context.session_id = SessionId::new(999);
    assert_eq!(
        FrameEnvelope::validate_rpc_stream_sequence(&[metadata.clone(), wrong_context])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );

    let wrong_kind = envelope(PayloadKind::Error, b"diagnostic".to_vec());
    assert_eq!(
        FrameEnvelope::validate_rpc_stream_sequence(&[metadata, wrong_kind])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn zero_row_or_mutation_only_completion_requires_explicit_metadata_policy() {
    let metadata = envelope(PayloadKind::RpcMetadata, b"zero-row-policy".to_vec());
    let completion = envelope(PayloadKind::RpcCompletion, Vec::new());
    let sequence = [metadata, completion];

    assert_eq!(
        FrameEnvelope::validate_rpc_stream_sequence(&sequence)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
    assert_eq!(
        FrameEnvelope::validate_rpc_stream_sequence_with_metadata_policy(
            &sequence,
            RpcResultStreamMetadataPolicy::RowBatchRequired,
        )
        .unwrap_err()
        .kind(),
        AndromedaErrorKind::Protocol
    );
    assert!(
        FrameEnvelope::validate_rpc_stream_sequence_with_metadata_policy(
            &sequence,
            RpcResultStreamMetadataPolicy::ZeroRowCompletionAllowed,
        )
        .is_ok()
    );
    assert!(
        FrameEnvelope::validate_rpc_stream_sequence_with_metadata_policy(
            &sequence,
            RpcResultStreamMetadataPolicy::MutationOnly,
        )
        .is_ok()
    );
}

#[test]
fn metadata_before_payload_invariant_is_consistent_across_proto_and_proto_wire() {
    let proto_metadata = envelope(PayloadKind::RpcMetadata, b"columns".to_vec());
    let proto_batch = envelope(PayloadKind::RpcBatch, b"row".to_vec());
    let proto_completion = envelope(PayloadKind::RpcCompletion, Vec::new());

    let wire_metadata = wire_envelope(WirePayloadKind::RpcMetadata, b"columns".to_vec());
    let wire_batch = wire_envelope(WirePayloadKind::RpcBatch, b"row".to_vec());
    let wire_completion = wire_envelope(WirePayloadKind::RpcCompletion, Vec::new());

    assert!(
        FrameEnvelope::validate_rpc_stream_sequence(&[
            proto_metadata.clone(),
            proto_batch.clone(),
            proto_completion.clone(),
        ])
        .is_ok()
    );
    assert!(
        WireFrameEnvelope::validate_rpc_stream_sequence_with_metadata_policy(
            &[
                wire_metadata.clone(),
                wire_batch.clone(),
                wire_completion.clone(),
            ],
            WireRpcResultStreamMetadataPolicy::RowBatchRequired,
        )
        .is_ok()
    );
    assert_eq!(
        FrameEnvelope::validate_rpc_stream_sequence(&[
            proto_batch.clone(),
            proto_metadata.clone(),
            proto_completion.clone(),
        ])
        .unwrap_err()
        .kind(),
        AndromedaErrorKind::Protocol
    );
    assert_eq!(
        WireFrameEnvelope::validate_rpc_stream_sequence_with_metadata_policy(
            &[
                wire_batch.clone(),
                wire_metadata.clone(),
                wire_completion.clone(),
            ],
            WireRpcResultStreamMetadataPolicy::RowBatchRequired,
        )
        .unwrap_err()
        .kind(),
        AndromedaErrorKind::Protocol
    );
    assert_eq!(
        proto_batch.payload_kind.wire_code(),
        wire_batch.payload_kind.wire_code()
    );
    assert_eq!(
        proto_metadata.payload_kind.wire_code(),
        wire_metadata.payload_kind.wire_code()
    );
    assert_eq!(
        proto_completion.payload_kind.wire_code(),
        wire_completion.payload_kind.wire_code()
    );
}

fn wire_envelope(kind: WirePayloadKind, payload: impl Into<Vec<u8>>) -> WireFrameEnvelope {
    WireFrameEnvelope {
        protocol_version: WireProtocolVersion::V1,
        contract_hash: ContractHash::test_vector(7),
        catalog_version: CatalogVersion::new(11),
        request_id: RequestId::new(101),
        session_id: SessionId::new(202),
        tx_id: Some(TransactionId::new(303)),
        payload_kind: kind,
        payload: payload.into(),
    }
}
