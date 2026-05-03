use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, RequestId, SessionId, TransactionId,
};

use crate::{FrameFamily, StreamRole};

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
            }
            Self::Error => FrameFamily::Diagnostic,
            Self::TelemetrySoftSignal => FrameFamily::Telemetry,
        }
    }

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

    pub const fn requires_non_empty_payload(self) -> bool {
        matches!(self, Self::RpcExecuteRequest | Self::RpcBatch)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultStreamMetadataPolicy {
    RowBatchRequired,
    ZeroRowCompletionAllowed,
    MutationOnly,
}

impl ResultStreamMetadataPolicy {
    pub const fn allows_completion_without_batch(self) -> bool {
        matches!(self, Self::ZeroRowCompletionAllowed | Self::MutationOnly)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultStreamSequence {
    metadata_policy: ResultStreamMetadataPolicy,
    request_context: Option<(RequestId, SessionId, Option<TransactionId>)>,
    saw_metadata: bool,
    saw_batch: bool,
    completed: bool,
}

impl ResultStreamSequence {
    pub const fn new() -> Self {
        Self::new_with_metadata_policy(ResultStreamMetadataPolicy::RowBatchRequired)
    }

    pub const fn new_with_metadata_policy(metadata_policy: ResultStreamMetadataPolicy) -> Self {
        Self {
            metadata_policy,
            request_context: None,
            saw_metadata: false,
            saw_batch: false,
            completed: false,
        }
    }

    pub fn accept(&mut self, frame: &FrameBytes) -> AndromedaResult<()> {
        frame.validate(StreamRole::ResultUnidirectional)?;
        self.validate_context(frame.header)?;

        match frame.header.frame_type {
            FrameType::RpcMetadata => {
                if self.saw_metadata || self.saw_batch || self.completed {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Protocol,
                        "RPC metadata must be the first result-stream frame",
                    ));
                }

                self.saw_metadata = true;
            }
            FrameType::RpcBatch => {
                if !self.saw_metadata {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Protocol,
                        "RPC metadata must precede RPC batch frames",
                    ));
                }

                if self.completed {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Protocol,
                        "RPC batch must not follow completion",
                    ));
                }

                self.saw_batch = true;
            }
            FrameType::RpcCompletion => {
                if !self.saw_metadata {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Protocol,
                        "RPC completion requires prior metadata",
                    ));
                }

                if !self.saw_batch && !self.metadata_policy.allows_completion_without_batch() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Protocol,
                        "RPC completion without a batch requires explicit metadata policy",
                    ));
                }

                if self.completed {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Protocol,
                        "RPC completion must appear once",
                    ));
                }

                self.completed = true;
            }
            _ => {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Protocol,
                    "result-stream sequence accepts only RPC metadata, batch, and completion",
                ));
            }
        }

        Ok(())
    }

    pub fn is_complete(self) -> bool {
        self.saw_metadata
            && self.completed
            && (self.saw_batch || self.metadata_policy.allows_completion_without_batch())
    }

    fn validate_context(&mut self, header: FrameHeader) -> AndromedaResult<()> {
        let current_context = (header.request_id, header.session_id, header.tx_id);

        match self.request_context {
            Some(expected_context) if expected_context != current_context => {
                Err(AndromedaError::new(
                    AndromedaErrorKind::Protocol,
                    "result-stream sequence changed request context",
                ))
            }
            None => {
                self.request_context = Some(current_context);
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

impl Default for ResultStreamSequence {
    fn default() -> Self {
        Self::new()
    }
}

pub fn validate_result_stream_sequence(frames: &[FrameBytes]) -> AndromedaResult<()> {
    validate_result_stream_sequence_with_metadata_policy(
        frames,
        ResultStreamMetadataPolicy::RowBatchRequired,
    )
}

pub fn validate_result_stream_sequence_with_metadata_policy(
    frames: &[FrameBytes],
    metadata_policy: ResultStreamMetadataPolicy,
) -> AndromedaResult<()> {
    let mut sequence = ResultStreamSequence::new_with_metadata_policy(metadata_policy);

    for frame in frames {
        sequence.accept(frame)?;
    }

    if !sequence.is_complete() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            "result-stream sequence is incomplete",
        ));
    }

    Ok(())
}

pub fn validate_single_frame_on_stream(
    frame: &FrameBytes,
    stream_role: StreamRole,
) -> AndromedaResult<()> {
    frame.validate(stream_role)
}

pub fn validate_frame_sequence(
    frames: &[FrameBytes],
    stream_role: StreamRole,
) -> AndromedaResult<()> {
    match stream_role {
        StreamRole::ResultUnidirectional => validate_result_stream_sequence(frames),
        _ => {
            for frame in frames {
                validate_single_frame_on_stream(frame, stream_role)?;
            }

            Ok(())
        }
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
    fn datagram_is_only_for_soft_telemetry() {
        assert!(FrameType::TelemetrySoftSignal.allows_datagram());
        assert!(!FrameType::RpcBatch.allows_datagram());
        assert!(FrameType::RpcBatch.requires_reliable_stream());
    }

    #[test]
    fn frame_type_declares_locked_wire_codes_and_families() {
        assert_eq!(FrameType::RpcBatch.wire_code(), 7);
        assert_eq!(FrameType::try_from(7).unwrap(), FrameType::RpcBatch);
        assert_eq!(
            FrameType::RpcBatch.frame_family(),
            FrameFamily::RpcResultStream
        );
        assert_eq!(
            FrameType::TelemetrySoftSignal.frame_family(),
            FrameFamily::Telemetry
        );
        assert_eq!(
            FrameType::try_from(10).unwrap_err().kind(),
            AndromedaErrorKind::Protocol
        );
    }

    #[test]
    fn frame_type_payload_codes_are_locked_for_proto_payload_kinds() {
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
    fn frame_role_policy_rejects_application_admin_mixing() {
        let header = header(FrameType::RpcBatch);

        assert_eq!(
            header
                .validate_transport_policy(StreamRole::CommandBidirectional)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );
        assert!(header
            .validate_transport_policy(StreamRole::ResultUnidirectional)
            .is_ok());
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
    }

    #[test]
    fn result_stream_sequence_accepts_metadata_batch_completion() {
        let frames = vec![
            FrameBytes {
                header: header_with_len(FrameType::RpcMetadata, 4),
                payload: b"meta".to_vec(),
            },
            FrameBytes {
                header: header_with_len(FrameType::RpcBatch, 3),
                payload: b"row".to_vec(),
            },
            FrameBytes {
                header: header_with_len(FrameType::RpcCompletion, 0),
                payload: Vec::new(),
            },
        ];

        assert!(validate_result_stream_sequence(&frames).is_ok());
        assert!(validate_frame_sequence(&frames, StreamRole::ResultUnidirectional).is_ok());
    }

    #[test]
    fn result_stream_sequence_rejects_metadata_completion_without_batch() {
        let frames = vec![
            FrameBytes {
                header: header_with_len(FrameType::RpcMetadata, 4),
                payload: b"meta".to_vec(),
            },
            FrameBytes {
                header: header_with_len(FrameType::RpcCompletion, 0),
                payload: Vec::new(),
            },
        ];

        assert_eq!(
            validate_result_stream_sequence(&frames).unwrap_err().kind(),
            AndromedaErrorKind::Protocol
        );
    }

    #[test]
    fn result_stream_sequence_rejects_batch_before_metadata() {
        let frames = vec![FrameBytes {
            header: header_with_len(FrameType::RpcBatch, 3),
            payload: b"row".to_vec(),
        }];

        assert_eq!(
            validate_result_stream_sequence(&frames).unwrap_err().kind(),
            AndromedaErrorKind::Protocol
        );
    }

    #[test]
    fn result_stream_sequence_rejects_context_changes() {
        let metadata = FrameBytes {
            header: header_with_len(FrameType::RpcMetadata, 4),
            payload: b"meta".to_vec(),
        };
        let mut batch = FrameBytes {
            header: header_with_len(FrameType::RpcBatch, 3),
            payload: b"row".to_vec(),
        };
        batch.header.request_id = RequestId::new(99);

        assert_eq!(
            validate_result_stream_sequence(&[metadata, batch])
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );
    }
}
