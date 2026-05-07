use andromeda_core::{AndromedaErrorKind, SessionId};
use andromeda_proto::{FrameEnvelope, PayloadKind, RpcResultStreamMetadataPolicy};

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
