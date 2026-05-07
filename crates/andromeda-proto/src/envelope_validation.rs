//! RPC stream sequence validation.
//!
//! This module validates the structure and ordering constraints of RPC result streams,
//! ensuring protocol compliance for metadata, batches, and completions.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, ContractHash, RequestId, SessionId, TransactionId};

use super::envelope_frame::FrameEnvelope;
use crate::PayloadKind;

/// Metadata policy for RPC result streams.
///
/// Determines whether a result stream may have a completion without preceding batch data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcResultStreamMetadataPolicy {
    /// Row batch is required before completion
    RowBatchRequired,
    /// Completion without batch is allowed (empty result set)
    ZeroRowCompletionAllowed,
    /// Only mutations, no row stream expected
    MutationOnly,
}

impl RpcResultStreamMetadataPolicy {
    /// Returns true if completion without a batch is allowed by this policy.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// assert!(!RpcResultStreamMetadataPolicy::RowBatchRequired
    ///     .allows_completion_without_batch());
    /// assert!(RpcResultStreamMetadataPolicy::ZeroRowCompletionAllowed
    ///     .allows_completion_without_batch());
    /// ```
    pub const fn allows_completion_without_batch(self) -> bool {
        matches!(self, Self::ZeroRowCompletionAllowed | Self::MutationOnly)
    }
}

impl FrameEnvelope {
    /// Validates that a sequence of envelopes forms a valid RPC result stream.
    ///
    /// Default validation requires: metadata → batch* → completion
    ///
    /// # Errors
    ///
    /// - Missing metadata or completion
    /// - Metadata not first
    /// - Batch without metadata
    /// - Completion appears multiple times
    /// - Request context changes within stream
    ///
    /// # Examples
    ///
    /// ```ignore
    /// FrameEnvelope::validate_rpc_stream_sequence(&[metadata, batch, completion])?;
    /// ```
    pub fn validate_rpc_stream_sequence(sequence: &[Self]) -> AndromedaResult<()> {
        Self::validate_rpc_stream_sequence_with_metadata_policy(
            sequence,
            RpcResultStreamMetadataPolicy::RowBatchRequired,
        )
    }

    /// Validates an RPC stream sequence with a custom metadata policy.
    ///
    /// Allows customization of requirements based on the operation type.
    ///
    /// # Errors
    ///
    /// Returns protocol-level errors for ordering or context violations.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// FrameEnvelope::validate_rpc_stream_sequence_with_metadata_policy(
    ///     &[metadata, completion],
    ///     RpcResultStreamMetadataPolicy::ZeroRowCompletionAllowed,
    /// )?;
    /// ```
    pub fn validate_rpc_stream_sequence_with_metadata_policy(
        sequence: &[Self],
        metadata_policy: RpcResultStreamMetadataPolicy,
    ) -> AndromedaResult<()> {
        let mut saw_metadata = false;
        let mut saw_batch = false;
        let mut saw_completion = false;
        let mut request_context: Option<(
            RequestId,
            SessionId,
            Option<TransactionId>,
            ContractHash,
            CatalogVersion,
        )> = None;

        for envelope in sequence {
            envelope.validate()?;

            if matches!(
                envelope.payload_kind,
                PayloadKind::RpcMetadata | PayloadKind::RpcBatch | PayloadKind::RpcCompletion
            ) {
                let current_context = (
                    envelope.request_id,
                    envelope.session_id,
                    envelope.tx_id,
                    envelope.contract_hash,
                    envelope.catalog_version,
                );

                match request_context {
                    Some(expected_context) if expected_context != current_context => {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Protocol,
                            "RPC stream sequence changed request context",
                        ));
                    }
                    None => request_context = Some(current_context),
                    _ => {}
                }
            }

            match envelope.payload_kind {
                PayloadKind::RpcMetadata => {
                    if saw_completion {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Protocol,
                            "RPC metadata must be the first result-stream envelope",
                        ));
                    }

                    if saw_metadata || saw_batch {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Protocol,
                            "RPC metadata must be the first result-stream envelope",
                        ));
                    }

                    saw_metadata = true;
                }
                PayloadKind::RpcBatch => {
                    if !saw_metadata {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Protocol,
                            "RPC metadata must precede RPC batch payloads",
                        ));
                    }

                    if saw_completion {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Protocol,
                            "RPC batch must not follow completion",
                        ));
                    }

                    saw_batch = true;
                }
                PayloadKind::RpcCompletion => {
                    if !saw_metadata {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Protocol,
                            "RPC completion requires prior metadata",
                        ));
                    }

                    if !saw_batch && !metadata_policy.allows_completion_without_batch() {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Protocol,
                            "RPC completion without a batch requires explicit metadata policy",
                        ));
                    }

                    if saw_completion {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Protocol,
                            "RPC completion must appear once",
                        ));
                    }

                    saw_completion = true;
                }
                _ => {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Protocol,
                        "RPC stream sequence accepts only metadata, batch, and completion",
                    ));
                }
            }
        }

        if !saw_metadata
            || !saw_completion
            || (!saw_batch && !metadata_policy.allows_completion_without_batch())
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "RPC stream sequence is incomplete",
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(byte: u8) -> ContractHash {
        ContractHash::test_vector(byte)
    }

    fn envelope(payload_kind: PayloadKind, payload: Vec<u8>) -> FrameEnvelope {
        FrameEnvelope {
            protocol_version: crate::ProtocolVersion::V1,
            contract_hash: hash(7),
            catalog_version: andromeda_types::CatalogVersion::new(1),
            request_id: RequestId::new(10),
            session_id: SessionId::new(20),
            tx_id: Some(TransactionId::new(30)),
            payload_kind,
            payload,
        }
    }

    #[test]
    fn rpc_stream_sequence_requires_metadata_before_batch() {
        let metadata = envelope(PayloadKind::RpcMetadata, b"columns".to_vec());
        let batch = envelope(PayloadKind::RpcBatch, b"row".to_vec());
        let completion = envelope(PayloadKind::RpcCompletion, Vec::new());

        assert!(
            FrameEnvelope::validate_rpc_stream_sequence(&[
                metadata.clone(),
                batch.clone(),
                completion
            ])
            .is_ok()
        );

        assert_eq!(
            FrameEnvelope::validate_rpc_stream_sequence(&[batch, metadata])
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );
    }

    #[test]
    fn rpc_stream_sequence_matches_quic_strict_completion_shape() {
        let metadata = envelope(PayloadKind::RpcMetadata, b"columns".to_vec());
        let batch = envelope(PayloadKind::RpcBatch, b"row".to_vec());
        let completion = envelope(PayloadKind::RpcCompletion, Vec::new());

        assert_eq!(
            FrameEnvelope::validate_rpc_stream_sequence(&[metadata.clone(), completion.clone()])
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );
        assert_eq!(
            FrameEnvelope::validate_rpc_stream_sequence(&[metadata.clone(), batch.clone()])
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );
        assert_eq!(
            FrameEnvelope::validate_rpc_stream_sequence(&[
                metadata.clone(),
                batch.clone(),
                completion.clone(),
                completion
            ])
            .unwrap_err()
            .kind(),
            AndromedaErrorKind::Protocol
        );
    }

    #[test]
    fn rpc_stream_sequence_rejects_context_changes() {
        let metadata = envelope(PayloadKind::RpcMetadata, b"columns".to_vec());
        let mut batch = envelope(PayloadKind::RpcBatch, b"row".to_vec());
        batch.request_id = RequestId::new(11);

        assert_eq!(
            FrameEnvelope::validate_rpc_stream_sequence(&[metadata, batch])
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );
    }
}
