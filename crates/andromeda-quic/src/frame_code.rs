//! Frame type codes and definitions.
//!
//! This module defines the QUIC frame type codes, frame families, and frame type enumeration
//! that form the foundation of the Andromeda QUIC protocol layer.

use andromeda_core::{AndromedaError, AndromedaErrorKind};

use super::{FrameFamily, StreamRole};

/// Hello frame type code (session initiation).
pub const HELLO_FRAME_CODE: u32 = 1;
/// Auth frame type code (authentication).
pub const AUTH_FRAME_CODE: u32 = 2;
/// Contract request frame type code.
pub const CONTRACT_REQUEST_FRAME_CODE: u32 = 3;
/// Contract response frame type code.
pub const CONTRACT_RESPONSE_FRAME_CODE: u32 = 4;
/// RPC execute request frame type code.
pub const RPC_EXECUTE_REQUEST_FRAME_CODE: u32 = 5;
/// RPC metadata frame type code.
pub const RPC_METADATA_FRAME_CODE: u32 = 6;
/// RPC batch frame type code (row data).
pub const RPC_BATCH_FRAME_CODE: u32 = 7;
/// RPC completion frame type code (end of result stream).
pub const RPC_COMPLETION_FRAME_CODE: u32 = 8;
/// Error frame type code (protocol error signaling).
pub const ERROR_FRAME_CODE: u32 = 9;
/// Telemetry soft signal frame type code (diagnostic datagram).
pub const TELEMETRY_SOFT_SIGNAL_FRAME_CODE: u32 = 100;

/// Maximum frame payload length (16 MiB).
pub const MAX_FRAME_PAYLOAD_LENGTH: u64 = 16 * 1024 * 1024;

/// Reserved frame flags mask (all flags are reserved).
pub const RESERVED_FRAME_FLAGS_MASK: u32 = u32::MAX;

/// Frame header CRC unchecked sentinel value.
pub const FRAME_HEADER_CRC_UNCHECKED: u32 = 0;

/// Wire frame type enumeration.
///
/// Represents all QUIC frame types in the Andromeda protocol.
/// Frame types are mapped to wire codes and frame families for routing and validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum FrameType {
    /// Session initiation frame.
    Hello = HELLO_FRAME_CODE,
    /// Authentication frame.
    Auth = AUTH_FRAME_CODE,
    /// Contract request frame.
    ContractRequest = CONTRACT_REQUEST_FRAME_CODE,
    /// Contract response frame.
    ContractResponse = CONTRACT_RESPONSE_FRAME_CODE,
    /// RPC execution request frame.
    RpcExecuteRequest = RPC_EXECUTE_REQUEST_FRAME_CODE,
    /// RPC metadata frame (column definitions).
    RpcMetadata = RPC_METADATA_FRAME_CODE,
    /// RPC batch frame (row data).
    RpcBatch = RPC_BATCH_FRAME_CODE,
    /// RPC completion frame (result stream terminator).
    RpcCompletion = RPC_COMPLETION_FRAME_CODE,
    /// Error frame (protocol error).
    Error = ERROR_FRAME_CODE,
    /// Telemetry soft signal frame (diagnostic datagram).
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
    /// Returns the wire code for this frame type.
    pub const fn wire_code(self) -> u32 {
        self as u32
    }

    /// Returns the frame family for this frame type.
    pub const fn frame_family(self) -> FrameFamily {
        match self {
            Self::Hello | Self::Auth => FrameFamily::SessionControl,
            Self::ContractRequest | Self::ContractResponse => FrameFamily::ContractControl,
            Self::RpcExecuteRequest => FrameFamily::RpcCommand,
            Self::RpcMetadata | Self::RpcBatch | Self::RpcCompletion => {
                FrameFamily::RpcResultStream
            }
            Self::Error => FrameFamily::Diagnostic,
            Self::TelemetrySoftSignal => FrameFamily::Telemetry,
        }
    }

    /// Returns the stream role required for this frame type.
    pub const fn stream_role(self) -> StreamRole {
        match self {
            Self::Hello | Self::Auth => StreamRole::SessionControl,
            Self::ContractRequest | Self::ContractResponse | Self::RpcExecuteRequest => {
                StreamRole::CommandBidirectional
            }
            Self::RpcMetadata | Self::RpcBatch | Self::RpcCompletion => {
                StreamRole::ResultUnidirectional
            }
            Self::Error => StreamRole::Diagnostic,
            Self::TelemetrySoftSignal => StreamRole::TelemetryDatagram,
        }
    }

    /// Returns true if this frame type allows datagram transport.
    pub const fn allows_datagram(self) -> bool {
        matches!(self, Self::TelemetrySoftSignal)
    }

    /// Returns true if this frame type requires reliable stream transport.
    pub const fn requires_reliable_stream(self) -> bool {
        !self.allows_datagram()
    }

    /// Returns true if this frame type requires a non-empty payload.
    pub const fn requires_non_empty_payload(self) -> bool {
        matches!(self, Self::RpcExecuteRequest | Self::RpcBatch)
    }

    /// Returns true if metadata must precede this frame type in a result stream.
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
        assert!(!FRAME_TYPE_PAYLOAD_CODE_LOCKSTEP
            .iter()
            .any(|(_, code)| *code == TELEMETRY_SOFT_SIGNAL_FRAME_CODE));
    }

    #[test]
    fn frame_type_allows_datagram_only_for_telemetry() {
        assert!(FrameType::TelemetrySoftSignal.allows_datagram());
        assert!(!FrameType::RpcBatch.allows_datagram());
        assert!(FrameType::RpcBatch.requires_reliable_stream());
    }
}
