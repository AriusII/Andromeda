//! Frame header and payload validation.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{RequestId, SessionId, TransactionId};

use super::frame_code::{FrameType, MAX_FRAME_PAYLOAD_LENGTH, RESERVED_FRAME_FLAGS_MASK};
use crate::StreamRole;

/// Frame header domain model.
///
/// The encoded wire header is fixed at `FRAME_CODEC_HEADER_LEN` bytes. This
/// Rust struct is not the wire layout because Rust may add padding.
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
    pub fn validate_reserved_flags(&self) -> AndromedaResult<()> {
        if self.flags & RESERVED_FRAME_FLAGS_MASK == 0 {
            return Ok(());
        }

        Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            "frame header contains reserved flags",
        ))
    }

    pub fn validate_max_payload_length(&self) -> AndromedaResult<()> {
        if self.payload_length <= MAX_FRAME_PAYLOAD_LENGTH {
            return Ok(());
        }

        Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            "frame payload length exceeds maximum",
        ))
    }

    pub fn validate_header_crc(&self, expected_crc: u32) -> AndromedaResult<()> {
        if self.header_crc == expected_crc {
            return Ok(());
        }

        Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            "frame header CRC mismatch",
        ))
    }

    pub fn validate_static_fields(&self) -> AndromedaResult<()> {
        self.validate_reserved_flags()?;
        self.validate_max_payload_length()
    }

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
        if stream_role.permits_family(self.frame_type.frame_family()) {
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
        self.header.validate_static_fields()?;
        self.header.validate_payload_length(self.payload.len())?;
        self.header.validate_transport_policy(stream_role)?;

        if self.header.frame_type.requires_non_empty_payload() && self.payload.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "frame type requires a non-empty payload",
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(frame_type: FrameType) -> FrameHeader {
        header_with_len(frame_type, 8)
    }

    fn header_with_len(frame_type: FrameType, payload_length: u64) -> FrameHeader {
        FrameHeader {
            frame_type,
            request_id: RequestId::new(1),
            session_id: SessionId::new(2),
            tx_id: None,
            payload_length,
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
    fn frame_header_rejects_reserved_flags_and_oversized_payloads() {
        let mut reserved_flags_header = header(FrameType::RpcExecuteRequest);
        reserved_flags_header.flags = 1;
        assert_eq!(
            reserved_flags_header
                .validate_static_fields()
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );

        let mut oversized_header = header(FrameType::RpcExecuteRequest);
        oversized_header.payload_length = MAX_FRAME_PAYLOAD_LENGTH + 1;
        assert_eq!(
            oversized_header
                .validate_static_fields()
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );
    }

    #[test]
    fn frame_header_crc_helper_rejects_mismatch() {
        let mut header = header(FrameType::RpcExecuteRequest);
        header.header_crc = 0xAABB_CCDD;

        assert!(header.validate_header_crc(0xAABB_CCDD).is_ok());
        assert_eq!(
            header.validate_header_crc(0xDEAD_BEEF).unwrap_err().kind(),
            AndromedaErrorKind::Protocol
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

    #[test]
    fn frame_bytes_rejects_empty_required_payloads() {
        let frame = FrameBytes {
            header: header_with_len(FrameType::RpcBatch, 0),
            payload: Vec::new(),
        };

        assert_eq!(
            frame
                .validate(StreamRole::ResultUnidirectional)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );

        let error_frame = FrameBytes {
            header: header_with_len(FrameType::Error, 0),
            payload: Vec::new(),
        };

        assert_eq!(
            error_frame
                .validate(StreamRole::Diagnostic)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );
    }
}
