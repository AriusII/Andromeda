//! Result stream sequence validation.
//!
//! This module enforces the strict ordering of frames in result streams:
//! metadata -> batch* -> completion

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, RequestId, SessionId, TransactionId,
};

use super::frame_code::FrameType;
use super::frame_struct::{FrameBytes, FrameHeader};
use crate::StreamRole;

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

/// State machine for validating result stream frame sequences.
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

pub fn validate_frame_sequence(
    frames: &[FrameBytes],
    stream_role: StreamRole,
) -> AndromedaResult<()> {
    match stream_role {
        StreamRole::ResultUnidirectional => validate_result_stream_sequence(frames),
        _ => {
            for frame in frames {
                frame.validate(stream_role)?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FrameBytes, FrameHeader};

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

    fn frame(frame_type: FrameType, payload: Vec<u8>) -> FrameBytes {
        FrameBytes {
            header: header_with_len(frame_type, payload.len() as u64),
            payload,
        }
    }

    #[test]
    fn result_stream_sequence_accepts_metadata_batch_completion() {
        let frames = vec![
            frame(FrameType::RpcMetadata, b"meta".to_vec()),
            frame(FrameType::RpcBatch, b"row".to_vec()),
            frame(FrameType::RpcCompletion, Vec::new()),
        ];

        assert!(validate_result_stream_sequence(&frames).is_ok());
    }

    #[test]
    fn result_stream_sequence_requires_completion() {
        let frames = vec![
            frame(FrameType::RpcMetadata, b"meta".to_vec()),
            frame(FrameType::RpcBatch, b"row".to_vec()),
        ];

        assert_eq!(
            validate_result_stream_sequence(&frames).unwrap_err().kind(),
            AndromedaErrorKind::Protocol
        );
    }
}
