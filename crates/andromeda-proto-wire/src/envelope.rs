use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, ContractHash, RequestId, SessionId, TransactionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolVersion {
    pub major: u32,
    pub minor: u32,
}

impl ProtocolVersion {
    pub const V1: Self = Self { major: 1, minor: 0 };
    pub const SUPPORTED_MAJOR: u32 = Self::V1.major;
    pub const SUPPORTED_MINOR: u32 = Self::V1.minor;

    pub fn validate(self) -> AndromedaResult<()> {
        if self.major == 0 {
            return protocol_error("protocol major version must be positive");
        }

        if self.major != Self::SUPPORTED_MAJOR {
            return protocol_error("unsupported protocol major version");
        }

        if self.minor > Self::SUPPORTED_MINOR {
            return protocol_error("unsupported protocol minor version");
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PayloadKind {
    Hello = 1,
    Auth = 2,
    ContractRequest = 3,
    ContractResponse = 4,
    RpcExecuteRequest = 5,
    RpcMetadata = 6,
    RpcBatch = 7,
    RpcCompletion = 8,
    Error = 9,
}

impl PayloadKind {
    pub const fn wire_code(self) -> u32 {
        self as u32
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
                    }
                    None => request_context = Some(current_context),
                    _ => {}
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
                }
                PayloadKind::RpcBatch => {
                    if !saw_metadata {
                        return protocol_error("RPC metadata must precede RPC batch payloads");
                    }

                    if saw_completion {
                        return protocol_error("RPC batch must not follow completion");
                    }

                    saw_batch = true;
                }
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
                }
                _ => {
                    return protocol_error(
                        "RPC stream sequence accepts only metadata, batch, and completion",
                    );
                }
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
}

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

fn contract_error<T>(message: &'static str) -> AndromedaResult<T> {
    Err(AndromedaError::new(AndromedaErrorKind::Contract, message))
}

fn protocol_error<T>(message: &'static str) -> AndromedaResult<T> {
    Err(AndromedaError::new(AndromedaErrorKind::Protocol, message))
}
