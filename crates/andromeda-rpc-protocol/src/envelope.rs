//! Typed RPC frame envelope domain model.

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, ContractHash, RequestId,
    SessionId, TransactionId,
};

use crate::frame_code::{
    AUTH_FRAME_CODE, CONTRACT_REQUEST_FRAME_CODE, CONTRACT_RESPONSE_FRAME_CODE, ERROR_FRAME_CODE,
    HELLO_FRAME_CODE, RPC_BATCH_FRAME_CODE, RPC_COMPLETION_FRAME_CODE,
    RPC_EXECUTE_REQUEST_FRAME_CODE, RPC_METADATA_FRAME_CODE,
};
use crate::frame_sequence::ResultStreamMetadataPolicy;
use crate::stream_types::FrameFamily;
use andromeda_procedure_contract::CompletionProtocolVersion as CompletionProtocolVersionContract;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolVersion {
    pub major: u32,
    pub minor: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletionProtocolVersion {
    pub major: u32,
    pub minor: u32,
}

/// Stable RPC completion envelope wire version.
///
/// The shape-level completion contract is owned by
/// `andromeda-procedure-contract`; this value binds that contract to the
/// negotiated RPC protocol version without involving protobuf generation.
pub const COMPLETION_ENVELOPE_VERSION: ProtocolVersion = ProtocolVersion::V1;

impl ProtocolVersion {
    pub const V1: Self = Self { major: 1, minor: 0 };
    pub const SUPPORTED_MAJOR: u32 = Self::V1.major;
    pub const SUPPORTED_MINOR: u32 = Self::V1.minor;

    pub const fn is_supported(self) -> bool {
        self.major == Self::SUPPORTED_MAJOR && self.minor == Self::SUPPORTED_MINOR
    }

    pub const fn is_compatible_with(self, other: Self) -> bool {
        self.major == other.major && other.minor <= self.minor
    }

    pub fn validate(self) -> AndromedaResult<()> {
        validate_protocol_version_fields(self.major, self.minor)
    }

    pub const fn completion_protocol_version(self) -> CompletionProtocolVersion {
        CompletionProtocolVersion {
            major: self.major,
            minor: self.minor,
        }
    }
}

impl CompletionProtocolVersion {
    pub const V1: Self = Self { major: 1, minor: 0 };

    pub fn validate(self) -> AndromedaResult<()> {
        validate_protocol_version_fields(self.major, self.minor)
    }
}

impl CompletionProtocolVersionContract for ProtocolVersion {
    fn validate_completion_protocol_version(self) -> AndromedaResult<()> {
        self.validate()
    }

    fn completion_protocol_major(self) -> u32 {
        self.major
    }

    fn completion_protocol_minor(self) -> u32 {
        self.minor
    }
}

impl CompletionProtocolVersionContract for CompletionProtocolVersion {
    fn validate_completion_protocol_version(self) -> AndromedaResult<()> {
        self.validate()
    }

    fn completion_protocol_major(self) -> u32 {
        self.major
    }

    fn completion_protocol_minor(self) -> u32 {
        self.minor
    }
}
impl From<ProtocolVersion> for CompletionProtocolVersion {
    fn from(value: ProtocolVersion) -> Self {
        value.completion_protocol_version()
    }
}

pub type PayloadFrameFamily = FrameFamily;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayloadFrameMapping {
    pub family: PayloadFrameFamily,
    pub transport_frame_code: u32,
}

pub const HELLO_WIRE_CODE: u32 = HELLO_FRAME_CODE;
pub const AUTH_WIRE_CODE: u32 = AUTH_FRAME_CODE;
pub const CONTRACT_REQUEST_WIRE_CODE: u32 = CONTRACT_REQUEST_FRAME_CODE;
pub const CONTRACT_RESPONSE_WIRE_CODE: u32 = CONTRACT_RESPONSE_FRAME_CODE;
pub const RPC_EXECUTE_REQUEST_WIRE_CODE: u32 = RPC_EXECUTE_REQUEST_FRAME_CODE;
pub const RPC_METADATA_WIRE_CODE: u32 = RPC_METADATA_FRAME_CODE;
pub const RPC_BATCH_WIRE_CODE: u32 = RPC_BATCH_FRAME_CODE;
pub const RPC_COMPLETION_WIRE_CODE: u32 = RPC_COMPLETION_FRAME_CODE;
pub const ERROR_WIRE_CODE: u32 = ERROR_FRAME_CODE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PayloadKind {
    Hello = HELLO_WIRE_CODE,
    Auth = AUTH_WIRE_CODE,
    ContractRequest = CONTRACT_REQUEST_WIRE_CODE,
    ContractResponse = CONTRACT_RESPONSE_WIRE_CODE,
    RpcExecuteRequest = RPC_EXECUTE_REQUEST_WIRE_CODE,
    RpcMetadata = RPC_METADATA_WIRE_CODE,
    RpcBatch = RPC_BATCH_WIRE_CODE,
    RpcCompletion = RPC_COMPLETION_WIRE_CODE,
    Error = ERROR_WIRE_CODE,
}

pub const PAYLOAD_KIND_TRANSPORT_CODE_LOCKSTEP: &[(PayloadKind, u32)] = &[
    (PayloadKind::Hello, HELLO_WIRE_CODE),
    (PayloadKind::Auth, AUTH_WIRE_CODE),
    (PayloadKind::ContractRequest, CONTRACT_REQUEST_WIRE_CODE),
    (PayloadKind::ContractResponse, CONTRACT_RESPONSE_WIRE_CODE),
    (
        PayloadKind::RpcExecuteRequest,
        RPC_EXECUTE_REQUEST_WIRE_CODE,
    ),
    (PayloadKind::RpcMetadata, RPC_METADATA_WIRE_CODE),
    (PayloadKind::RpcBatch, RPC_BATCH_WIRE_CODE),
    (PayloadKind::RpcCompletion, RPC_COMPLETION_WIRE_CODE),
    (PayloadKind::Error, ERROR_WIRE_CODE),
];

impl PayloadKind {
    pub const fn wire_code(self) -> u32 {
        self as u32
    }

    pub const fn frame_family(self) -> PayloadFrameFamily {
        match self {
            Self::Hello | Self::Auth => FrameFamily::SessionControl,
            Self::ContractRequest | Self::ContractResponse => FrameFamily::ContractControl,
            Self::RpcExecuteRequest => FrameFamily::RpcCommand,
            Self::RpcMetadata | Self::RpcBatch | Self::RpcCompletion => {
                FrameFamily::RpcResultStream
            },
            Self::Error => FrameFamily::Diagnostic,
        }
    }

    pub const fn frame_mapping(self) -> PayloadFrameMapping {
        PayloadFrameMapping {
            family: self.frame_family(),
            transport_frame_code: self.wire_code(),
        }
    }

    pub const fn requires_contract_hash(self) -> bool {
        matches!(
            self,
            Self::RpcExecuteRequest | Self::RpcMetadata | Self::RpcBatch | Self::RpcCompletion
        )
    }

    pub const fn requires_non_empty_payload(self) -> bool {
        matches!(self, Self::RpcExecuteRequest | Self::RpcBatch | Self::Error)
    }

    pub const fn metadata_must_precede_payload(self) -> bool {
        matches!(self, Self::RpcBatch)
    }
}

impl TryFrom<u32> for PayloadKind {
    type Error = AndromedaError;

    fn try_from(value: u32) -> Result<Self, AndromedaError> {
        match value {
            1 => Ok(Self::Hello),
            2 => Ok(Self::Auth),
            3 => Ok(Self::ContractRequest),
            4 => Ok(Self::ContractResponse),
            5 => Ok(Self::RpcExecuteRequest),
            6 => Ok(Self::RpcMetadata),
            7 => Ok(Self::RpcBatch),
            8 => Ok(Self::RpcCompletion),
            9 => Ok(Self::Error),
            _ => protocol_error("unknown Protobuf payload kind"),
        }
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

        contract_error("ContractHash mismatch")
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.protocol_version.validate()?;

        if self.payload_kind.requires_contract_hash() && self.contract_hash.is_zero() {
            return contract_error("payload kind requires a nonzero ContractHash");
        }

        if self.payload_kind.requires_contract_hash() {
            if self.catalog_version.get() == 0 {
                return contract_error(
                    "contract-bound payload kind requires a nonzero CatalogVersion",
                );
            }

            if self.request_id.get() == 0 {
                return protocol_error("contract-bound payload kind requires a nonzero RequestId");
            }

            if self.session_id.get() == 0 {
                return protocol_error("contract-bound payload kind requires a nonzero SessionId");
            }
        }

        if self.payload_kind.requires_non_empty_payload() && self.payload.is_empty() {
            return protocol_error("payload kind requires a non-empty payload");
        }

        Ok(())
    }

    pub fn validated(self) -> AndromedaResult<Self> {
        self.validate()?;
        Ok(self)
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
                        return protocol_error("RPC stream sequence changed request context");
                    },
                    None => request_context = Some(current_context),
                    _ => {},
                }
            }

            match envelope.payload_kind {
                PayloadKind::RpcMetadata => {
                    if saw_completion || saw_metadata || saw_batch {
                        return protocol_error(
                            "RPC metadata must be the first result-stream envelope",
                        );
                    }

                    saw_metadata = true;
                },
                PayloadKind::RpcBatch => {
                    if !saw_metadata {
                        return protocol_error("RPC metadata must precede RPC batch payloads");
                    }

                    if saw_completion {
                        return protocol_error("RPC batch must not follow completion");
                    }

                    saw_batch = true;
                },
                PayloadKind::RpcCompletion => {
                    if !saw_metadata {
                        return protocol_error("RPC completion requires prior metadata");
                    }

                    if !saw_batch && !metadata_policy.allows_completion_without_batch() {
                        return protocol_error(
                            "RPC completion without a batch requires explicit metadata policy",
                        );
                    }

                    if saw_completion {
                        return protocol_error("RPC completion must appear once");
                    }

                    saw_completion = true;
                },
                _ => {
                    return protocol_error(
                        "RPC stream sequence accepts only metadata, batch, and completion",
                    );
                },
            }
        }

        if !saw_metadata
            || !saw_completion
            || (!saw_batch && !metadata_policy.allows_completion_without_batch())
        {
            return protocol_error("RPC stream sequence is incomplete");
        }

        Ok(())
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

pub type RpcResultStreamMetadataPolicy = ResultStreamMetadataPolicy;

fn contract_error<T>(message: &'static str) -> AndromedaResult<T> {
    Err(AndromedaError::new(AndromedaErrorKind::Contract, message))
}

fn protocol_error<T>(message: &'static str) -> AndromedaResult<T> {
    Err(AndromedaError::new(AndromedaErrorKind::Protocol, message))
}

fn validate_protocol_version_fields(major: u32, minor: u32) -> AndromedaResult<()> {
    if major == 0 {
        return protocol_error("protocol major version must be positive");
    }

    if major != ProtocolVersion::SUPPORTED_MAJOR {
        return protocol_error("unsupported protocol major version");
    }

    if minor > ProtocolVersion::SUPPORTED_MINOR {
        return protocol_error("unsupported protocol minor version");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(byte: u8) -> ContractHash {
        ContractHash::test_vector(byte)
    }

    #[test]
    fn protocol_version_accepts_only_locked_v1_0_contract() {
        assert!(ProtocolVersion::V1.validate().is_ok());
        assert!(ProtocolVersion::V1.is_supported());
        assert_eq!(
            ProtocolVersion::V1.completion_protocol_version(),
            CompletionProtocolVersion::V1
        );
        assert!(CompletionProtocolVersion::V1.validate().is_ok());

        assert_eq!(
            ProtocolVersion { major: 1, minor: 1 }
                .validate()
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );

        assert_eq!(
            ProtocolVersion { major: 2, minor: 0 }
                .validate()
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );

        assert_eq!(
            CompletionProtocolVersion { major: 0, minor: 0 }
                .validate()
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );
    }

    #[test]
    fn payload_kind_transport_codes_are_locked() {
        for (payload_kind, transport_frame_code) in PAYLOAD_KIND_TRANSPORT_CODE_LOCKSTEP {
            assert_eq!(payload_kind.wire_code(), *transport_frame_code);
            assert_eq!(
                PayloadKind::try_from(*transport_frame_code).unwrap(),
                *payload_kind
            );
            assert_eq!(
                payload_kind.frame_mapping().transport_frame_code,
                *transport_frame_code
            );
        }
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
    fn rpc_stream_sequence_requires_metadata_before_batch() {
        let metadata = FrameEnvelope {
            protocol_version: ProtocolVersion::V1,
            contract_hash: hash(7),
            catalog_version: CatalogVersion::new(1),
            request_id: RequestId::new(10),
            session_id: SessionId::new(20),
            tx_id: Some(TransactionId::new(30)),
            payload_kind: PayloadKind::RpcMetadata,
            payload: b"columns".to_vec(),
        };
        let batch = FrameEnvelope {
            payload_kind: PayloadKind::RpcBatch,
            payload: b"row".to_vec(),
            ..metadata.clone()
        };
        let completion = FrameEnvelope {
            payload_kind: PayloadKind::RpcCompletion,
            payload: Vec::new(),
            ..metadata.clone()
        };

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
}
