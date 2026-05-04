//! Payload kind discrimination and frame family classification.
//!
//! This module defines payload types and their wire representation codes.

use andromeda_core::{AndromedaError, AndromedaErrorKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadFrameFamily {
    SessionControl,
    ContractControl,
    RpcCommand,
    RpcResultStream,
    Diagnostic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayloadFrameMapping {
    pub family: PayloadFrameFamily,
    pub transport_frame_code: u32,
}

pub const HELLO_WIRE_CODE: u32 = 1;
pub const AUTH_WIRE_CODE: u32 = 2;
pub const CONTRACT_REQUEST_WIRE_CODE: u32 = 3;
pub const CONTRACT_RESPONSE_WIRE_CODE: u32 = 4;
pub const RPC_EXECUTE_REQUEST_WIRE_CODE: u32 = 5;
pub const RPC_METADATA_WIRE_CODE: u32 = 6;
pub const RPC_BATCH_WIRE_CODE: u32 = 7;
pub const RPC_COMPLETION_WIRE_CODE: u32 = 8;
pub const ERROR_WIRE_CODE: u32 = 9;

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
            Self::Hello | Self::Auth => PayloadFrameFamily::SessionControl,
            Self::ContractRequest | Self::ContractResponse => PayloadFrameFamily::ContractControl,
            Self::RpcExecuteRequest => PayloadFrameFamily::RpcCommand,
            Self::RpcMetadata | Self::RpcBatch | Self::RpcCompletion => {
                PayloadFrameFamily::RpcResultStream
            }
            Self::Error => PayloadFrameFamily::Diagnostic,
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
        matches!(self, Self::RpcExecuteRequest | Self::RpcBatch)
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
            _ => Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "unknown Protobuf payload kind",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_kind_uses_locked_v0_family_values() {
        assert_eq!(
            PayloadKind::try_from(HELLO_WIRE_CODE).unwrap(),
            PayloadKind::Hello
        );
        assert_eq!(
            PayloadKind::try_from(RPC_EXECUTE_REQUEST_WIRE_CODE).unwrap(),
            PayloadKind::RpcExecuteRequest
        );
        assert_eq!(
            PayloadKind::try_from(ERROR_WIRE_CODE).unwrap(),
            PayloadKind::Error
        );
        assert_eq!(
            PayloadKind::try_from(10).unwrap_err().kind(),
            AndromedaErrorKind::Protocol
        );
    }

    #[test]
    fn payload_kind_transport_codes_are_locked_for_quic_frame_types() {
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
    fn payload_kind_declares_transport_mapping_without_quic_dependency() {
        assert_eq!(
            PayloadKind::Hello.frame_mapping(),
            PayloadFrameMapping {
                family: PayloadFrameFamily::SessionControl,
                transport_frame_code: 1,
            }
        );
        assert_eq!(
            PayloadKind::RpcBatch.frame_mapping(),
            PayloadFrameMapping {
                family: PayloadFrameFamily::RpcResultStream,
                transport_frame_code: 7,
            }
        );
        assert_eq!(
            PayloadKind::Error.frame_mapping().family,
            PayloadFrameFamily::Diagnostic
        );
    }

    #[test]
    fn payload_contract_and_body_requirements_are_explicit() {
        assert!(PayloadKind::RpcExecuteRequest.requires_contract_hash());
        assert!(PayloadKind::RpcBatch.requires_contract_hash());
        assert!(PayloadKind::RpcExecuteRequest.requires_non_empty_payload());
        assert!(PayloadKind::RpcBatch.requires_non_empty_payload());

        assert!(!PayloadKind::Error.requires_contract_hash());
        assert!(!PayloadKind::Error.requires_non_empty_payload());
    }
}
