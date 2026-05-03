use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, ContractHash, RequestId,
    SessionId, TransactionId,
};

use crate::{PayloadKind, ProtocolVersion};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcResultStreamMetadataPolicy {
    RowBatchRequired,
    ZeroRowCompletionAllowed,
    MutationOnly,
}

impl RpcResultStreamMetadataPolicy {
    pub const fn allows_completion_without_batch(self) -> bool {
        matches!(self, Self::ZeroRowCompletionAllowed | Self::MutationOnly)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameEnvelope {
    pub protocol_version: ProtocolVersion,
    pub contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
    pub request_id: RequestId,
    pub session_id: SessionId,
    pub tx_id: Option<TransactionId>,
    pub payload_kind: PayloadKind,
    pub payload: Vec<u8>,
}

impl FrameEnvelope {
    pub fn rpc_execute_request(
        contract_hash: ContractHash,
        catalog_version: CatalogVersion,
        request_id: RequestId,
        session_id: SessionId,
        tx_id: Option<TransactionId>,
        payload: impl Into<Vec<u8>>,
    ) -> AndromedaResult<Self> {
        Self {
            protocol_version: ProtocolVersion::V1,
            contract_hash,
            catalog_version,
            request_id,
            session_id,
            tx_id,
            payload_kind: PayloadKind::RpcExecuteRequest,
            payload: payload.into(),
        }
        .validated()
    }

    pub fn to_completion_envelope(&self, payload: impl Into<Vec<u8>>) -> AndromedaResult<Self> {
        self.related_envelope(PayloadKind::RpcCompletion, payload)
    }

    pub fn to_error_envelope(&self, payload: impl Into<Vec<u8>>) -> AndromedaResult<Self> {
        self.related_envelope(PayloadKind::Error, payload)
    }

    pub fn validate_contract_hash(&self, expected: ContractHash) -> AndromedaResult<()> {
        if self.contract_hash == expected {
            return Ok(());
        }

        Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "ContractHash mismatch",
        ))
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.protocol_version.validate()?;

        if self.payload_kind.requires_contract_hash() && self.contract_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "payload kind requires a nonzero ContractHash",
            ));
        }

        if self.payload_kind.requires_non_empty_payload() && self.payload.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "payload kind requires a non-empty payload",
            ));
        }

        Ok(())
    }

    pub fn validate_rpc_stream_sequence(sequence: &[Self]) -> AndromedaResult<()> {
        Self::validate_rpc_stream_sequence_with_metadata_policy(
            sequence,
            RpcResultStreamMetadataPolicy::RowBatchRequired,
        )
    }

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

    pub fn validated(self) -> AndromedaResult<Self> {
        self.validate()?;
        Ok(self)
    }

    fn related_envelope(
        &self,
        payload_kind: PayloadKind,
        payload: impl Into<Vec<u8>>,
    ) -> AndromedaResult<Self> {
        self.validate()?;

        Self {
            protocol_version: self.protocol_version,
            contract_hash: self.contract_hash,
            catalog_version: self.catalog_version,
            request_id: self.request_id,
            session_id: self.session_id,
            tx_id: self.tx_id,
            payload_kind,
            payload: payload.into(),
        }
        .validated()
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
            protocol_version: ProtocolVersion::V1,
            contract_hash: hash(7),
            catalog_version: CatalogVersion::new(1),
            request_id: RequestId::new(10),
            session_id: SessionId::new(20),
            tx_id: Some(TransactionId::new(30)),
            payload_kind,
            payload,
        }
    }

    #[test]
    fn rpc_payload_requires_nonzero_contract_hash() {
        let envelope = FrameEnvelope {
            protocol_version: ProtocolVersion::V1,
            contract_hash: ContractHash::zero(),
            catalog_version: CatalogVersion::new(1),
            request_id: RequestId::new(10),
            session_id: SessionId::new(20),
            tx_id: None,
            payload_kind: PayloadKind::RpcExecuteRequest,
            payload: Vec::new(),
        };

        assert_eq!(
            envelope.validate().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );
    }

    #[test]
    fn rpc_execute_helper_builds_validated_envelope() {
        let envelope = FrameEnvelope::rpc_execute_request(
            hash(7),
            CatalogVersion::new(3),
            RequestId::new(10),
            SessionId::new(20),
            Some(TransactionId::new(30)),
            b"ProductId=42;Quantity=3".to_vec(),
        )
        .unwrap();

        assert_eq!(envelope.protocol_version, ProtocolVersion::V1);
        assert_eq!(envelope.payload_kind, PayloadKind::RpcExecuteRequest);
        assert_eq!(envelope.contract_hash, hash(7));
        assert_eq!(envelope.catalog_version, CatalogVersion::new(3));
        assert_eq!(envelope.payload, b"ProductId=42;Quantity=3".to_vec());
        assert!(envelope.validate().is_ok());
    }

    #[test]
    fn rpc_execute_helper_rejects_zero_contract_hash() {
        let error = FrameEnvelope::rpc_execute_request(
            ContractHash::zero(),
            CatalogVersion::new(3),
            RequestId::new(10),
            SessionId::new(20),
            None,
            Vec::new(),
        )
        .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    }

    #[test]
    fn rpc_execute_and_batch_require_non_empty_payloads() {
        let execute_error = FrameEnvelope::rpc_execute_request(
            hash(7),
            CatalogVersion::new(3),
            RequestId::new(10),
            SessionId::new(20),
            None,
            Vec::new(),
        )
        .unwrap_err();

        assert_eq!(execute_error.kind(), AndromedaErrorKind::Protocol);

        let batch = envelope(PayloadKind::RpcBatch, Vec::new());
        assert_eq!(
            batch.validate().unwrap_err().kind(),
            AndromedaErrorKind::Protocol
        );
    }

    #[test]
    fn execute_envelope_derives_completion_and_error_envelopes() {
        let request = FrameEnvelope::rpc_execute_request(
            hash(9),
            CatalogVersion::new(4),
            RequestId::new(10),
            SessionId::new(20),
            Some(TransactionId::new(30)),
            b"reserve".to_vec(),
        )
        .unwrap();

        let completion = request.to_completion_envelope(vec![1]).unwrap();
        let error = request
            .to_error_envelope(b"permission denied".to_vec())
            .unwrap();

        assert_eq!(completion.payload_kind, PayloadKind::RpcCompletion);
        assert_eq!(completion.contract_hash, request.contract_hash);
        assert_eq!(completion.request_id, request.request_id);
        assert_eq!(completion.tx_id, request.tx_id);
        assert_eq!(completion.payload, vec![1]);
        assert!(completion.validate().is_ok());

        assert_eq!(error.payload_kind, PayloadKind::Error);
        assert_eq!(error.session_id, request.session_id);
        assert_eq!(error.payload, b"permission denied".to_vec());
        assert!(error.validate().is_ok());
    }

    #[test]
    fn hello_payload_can_run_before_contract_binding() {
        let envelope = FrameEnvelope {
            protocol_version: ProtocolVersion::V1,
            contract_hash: ContractHash::zero(),
            catalog_version: CatalogVersion::new(0),
            request_id: RequestId::new(10),
            session_id: SessionId::new(20),
            tx_id: None,
            payload_kind: PayloadKind::Hello,
            payload: Vec::new(),
        };

        assert!(envelope.validate().is_ok());
    }

    #[test]
    fn error_payload_is_allowed_without_contract_hash() {
        let envelope = FrameEnvelope {
            protocol_version: ProtocolVersion::V1,
            contract_hash: ContractHash::zero(),
            catalog_version: CatalogVersion::new(0),
            request_id: RequestId::new(10),
            session_id: SessionId::new(20),
            tx_id: None,
            payload_kind: PayloadKind::Error,
            payload: b"protocol failure".to_vec(),
        };

        assert!(envelope.validate().is_ok());
    }

    #[test]
    fn rpc_stream_sequence_requires_metadata_before_batch() {
        let metadata = envelope(PayloadKind::RpcMetadata, b"columns".to_vec());
        let batch = envelope(PayloadKind::RpcBatch, b"row".to_vec());
        let completion = envelope(PayloadKind::RpcCompletion, Vec::new());

        assert!(FrameEnvelope::validate_rpc_stream_sequence(&[
            metadata.clone(),
            batch.clone(),
            completion
        ])
        .is_ok());

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

    #[test]
    fn contract_hash_mismatch_is_rejected() {
        let expected = hash(1);
        let actual = hash(2);
        let envelope = FrameEnvelope {
            protocol_version: ProtocolVersion::V1,
            contract_hash: actual,
            catalog_version: CatalogVersion::new(1),
            request_id: RequestId::new(10),
            session_id: SessionId::new(20),
            tx_id: None,
            payload_kind: PayloadKind::RpcExecuteRequest,
            payload: Vec::new(),
        };

        assert_eq!(
            envelope
                .validate_contract_hash(expected)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Contract
        );
    }
}
