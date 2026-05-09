//! Frame type codes and routing metadata.

use andromeda_error::{AndromedaError, AndromedaErrorKind};

use super::{FrameFamily, StreamRole};

pub const HELLO_FRAME_CODE: u32 = 1;
pub const AUTH_FRAME_CODE: u32 = 2;
pub const CONTRACT_REQUEST_FRAME_CODE: u32 = 3;
pub const CONTRACT_RESPONSE_FRAME_CODE: u32 = 4;
pub const RPC_EXECUTE_REQUEST_FRAME_CODE: u32 = 5;
pub const RPC_METADATA_FRAME_CODE: u32 = 6;
pub const RPC_BATCH_FRAME_CODE: u32 = 7;
pub const RPC_COMPLETION_FRAME_CODE: u32 = 8;
pub const ERROR_FRAME_CODE: u32 = 9;
pub const TELEMETRY_SOFT_SIGNAL_FRAME_CODE: u32 = 100;

pub const MAX_FRAME_PAYLOAD_LENGTH: u64 = 16 * 1024 * 1024;

pub const RESERVED_FRAME_FLAGS_MASK: u32 = u32::MAX;

pub const FRAME_HEADER_CRC_UNCHECKED: u32 = 0;

/// Wire frame type enumeration mapped to routing families and stream roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum FrameType {
    Hello = HELLO_FRAME_CODE,
    Auth = AUTH_FRAME_CODE,
    ContractRequest = CONTRACT_REQUEST_FRAME_CODE,
    ContractResponse = CONTRACT_RESPONSE_FRAME_CODE,
    RpcExecuteRequest = RPC_EXECUTE_REQUEST_FRAME_CODE,
    RpcMetadata = RPC_METADATA_FRAME_CODE,
    RpcBatch = RPC_BATCH_FRAME_CODE,
    RpcCompletion = RPC_COMPLETION_FRAME_CODE,
    Error = ERROR_FRAME_CODE,
    TelemetrySoftSignal = TELEMETRY_SOFT_SIGNAL_FRAME_CODE,
}

/// Lockstep mapping of frame types to payload wire codes.
///
/// This constant ensures that frame type codes remain synchronized between the
/// QUIC transport layer and the protobuf payload layer.
pub const FRAME_TYPE_PAYLOAD_CODE_LOCKSTEP: &[(FrameType, u32)] = &[
    (FrameType::Hello, HELLO_FRAME_CODE),
    (FrameType::Auth, AUTH_FRAME_CODE),
    (FrameType::ContractRequest, CONTRACT_REQUEST_FRAME_CODE),
    (FrameType::ContractResponse, CONTRACT_RESPONSE_FRAME_CODE),
    (FrameType::RpcExecuteRequest, RPC_EXECUTE_REQUEST_FRAME_CODE),
    (FrameType::RpcMetadata, RPC_METADATA_FRAME_CODE),
    (FrameType::RpcBatch, RPC_BATCH_FRAME_CODE),
    (FrameType::RpcCompletion, RPC_COMPLETION_FRAME_CODE),
    (FrameType::Error, ERROR_FRAME_CODE),
];

impl FrameType {
    pub const fn wire_code(self) -> u32 {
        self as u32
    }

    pub const fn frame_family(self) -> FrameFamily {
        match self {
            Self::Hello | Self::Auth => FrameFamily::SessionControl,
            Self::ContractRequest | Self::ContractResponse => FrameFamily::ContractControl,
            Self::RpcExecuteRequest => FrameFamily::RpcCommand,
            Self::RpcMetadata | Self::RpcBatch | Self::RpcCompletion => {
                FrameFamily::RpcResultStream
            },
            Self::Error => FrameFamily::Diagnostic,
            Self::TelemetrySoftSignal => FrameFamily::Telemetry,
        }
    }

    pub const fn stream_role(self) -> StreamRole {
        match self {
            Self::Hello | Self::Auth => StreamRole::SessionControl,
            Self::ContractRequest | Self::ContractResponse | Self::RpcExecuteRequest => {
                StreamRole::CommandBidirectional
            },
            Self::RpcMetadata | Self::RpcBatch | Self::RpcCompletion => {
                StreamRole::ResultUnidirectional
            },
            Self::Error => StreamRole::Diagnostic,
            Self::TelemetrySoftSignal => StreamRole::TelemetryDatagram,
        }
    }

    pub const fn allows_datagram(self) -> bool {
        matches!(self, Self::TelemetrySoftSignal)
    }

    pub const fn requires_reliable_stream(self) -> bool {
        !self.allows_datagram()
    }

    pub const fn requires_non_empty_payload(self) -> bool {
        matches!(self, Self::RpcExecuteRequest | Self::RpcBatch | Self::Error)
    }

    pub const fn metadata_must_precede(self) -> bool {
        matches!(self, Self::RpcBatch)
    }
}

impl TryFrom<u32> for FrameType {
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
            100 => Ok(Self::TelemetrySoftSignal),
            _ => Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "unknown QUIC frame type code",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_type_declares_locked_wire_codes() {
        assert_eq!(FrameType::RpcBatch.wire_code(), 7);
        assert_eq!(FrameType::try_from(7).unwrap(), FrameType::RpcBatch);
        assert_eq!(
            FrameType::try_from(10).unwrap_err().kind(),
            AndromedaErrorKind::Protocol
        );
    }

    #[test]
    fn frame_type_payload_codes_locked_for_proto() {
        for (frame_type, payload_code) in FRAME_TYPE_PAYLOAD_CODE_LOCKSTEP {
            assert_eq!(frame_type.wire_code(), *payload_code);
            assert_eq!(FrameType::try_from(*payload_code).unwrap(), *frame_type);
        }

        assert_eq!(FrameType::TelemetrySoftSignal.wire_code(), 100);
        assert!(
            !FRAME_TYPE_PAYLOAD_CODE_LOCKSTEP
                .iter()
                .any(|(_, code)| *code == TELEMETRY_SOFT_SIGNAL_FRAME_CODE)
        );
    }

    #[test]
    fn frame_type_allows_datagram_only_for_telemetry() {
        assert!(FrameType::TelemetrySoftSignal.allows_datagram());
        assert!(!FrameType::RpcBatch.allows_datagram());
        assert!(FrameType::RpcBatch.requires_reliable_stream());
    }
}
