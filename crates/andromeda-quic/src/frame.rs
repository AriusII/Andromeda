use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, RequestId, SessionId, TransactionId,
};

use crate::StreamRole;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    Hello,
    Auth,
    ContractRequest,
    ContractResponse,
    RpcExecuteRequest,
    RpcMetadata,
    RpcBatch,
    RpcCompletion,
    Error,
    TelemetrySoftSignal,
}

impl FrameType {
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

    pub const fn allows_datagram(self) -> bool {
        matches!(self, Self::TelemetrySoftSignal)
    }

    pub const fn requires_reliable_stream(self) -> bool {
        !self.allows_datagram()
    }

    pub const fn metadata_must_precede(self) -> bool {
        matches!(self, Self::RpcBatch)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    pub frame_type: FrameType,
    pub request_id: RequestId,
    pub session_id: SessionId,
    pub tx_id: Option<TransactionId>,
    pub payload_length: u64,
    pub flags: u32,
    pub header_crc: u32,
}

impl FrameHeader {
    pub fn validate_payload_length(&self, actual_length: usize) -> AndromedaResult<()> {
        if self.payload_length == actual_length as u64 {
            return Ok(());
        }

        Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            "frame payload length mismatch",
        ))
    }

    pub fn validate_transport_policy(&self, stream_role: StreamRole) -> AndromedaResult<()> {
        if self.frame_type.stream_role() == stream_role {
            return Ok(());
        }

        Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            "frame type is not allowed on this stream role",
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameBytes {
    pub header: FrameHeader,
    pub payload: Vec<u8>,
}

impl FrameBytes {
    pub fn validate(&self, stream_role: StreamRole) -> AndromedaResult<()> {
        self.header.validate_payload_length(self.payload.len())?;
        self.header.validate_transport_policy(stream_role)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(frame_type: FrameType) -> FrameHeader {
        FrameHeader {
            frame_type,
            request_id: RequestId::new(1),
            session_id: SessionId::new(2),
            tx_id: None,
            payload_length: 8,
            flags: 0,
            header_crc: 0,
        }
    }

    #[test]
    fn frame_header_rejects_payload_length_mismatch() {
        let header = header(FrameType::RpcExecuteRequest);

        assert_eq!(
            header.validate_payload_length(7).unwrap_err().kind(),
            AndromedaErrorKind::Protocol
        );
        assert!(header.validate_payload_length(8).is_ok());
    }

    #[test]
    fn datagram_is_only_for_soft_telemetry() {
        assert!(FrameType::TelemetrySoftSignal.allows_datagram());
        assert!(!FrameType::RpcBatch.allows_datagram());
        assert!(FrameType::RpcBatch.requires_reliable_stream());
    }

    #[test]
    fn frame_role_policy_rejects_application_admin_mixing() {
        let header = header(FrameType::RpcBatch);

        assert_eq!(
            header
                .validate_transport_policy(StreamRole::CommandBidirectional)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );
        assert!(
            header
                .validate_transport_policy(StreamRole::ResultUnidirectional)
                .is_ok()
        );
    }

    #[test]
    fn frame_bytes_validate_combines_payload_length_and_stream_role() {
        let frame = FrameBytes {
            header: header(FrameType::RpcExecuteRequest),
            payload: vec![0; 8],
        };

        assert!(frame.validate(StreamRole::CommandBidirectional).is_ok());

        assert_eq!(
            frame
                .validate(StreamRole::ResultUnidirectional)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );
    }

    #[test]
    fn frame_bytes_validate_rejects_payload_length_mismatch() {
        let frame = FrameBytes {
            header: header(FrameType::RpcExecuteRequest),
            payload: vec![0; 7],
        };

        assert_eq!(
            frame
                .validate(StreamRole::CommandBidirectional)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );
    }
}
