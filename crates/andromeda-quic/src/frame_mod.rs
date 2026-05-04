//! QUIC frame protocol layer.
//!
//! This module organizes frame encoding, decoding, type definitions, and validation logic.

pub mod code;
pub mod codec;
pub mod sequence;
pub mod struct_;

pub use code::{
    FrameFamily, FrameType, StreamRole, AUTH_FRAME_CODE, CONTRACT_REQUEST_FRAME_CODE,
    CONTRACT_RESPONSE_FRAME_CODE, ERROR_FRAME_CODE, FRAME_HEADER_CRC_UNCHECKED,
    FRAME_TYPE_PAYLOAD_CODE_LOCKSTEP, HELLO_FRAME_CODE, MAX_FRAME_PAYLOAD_LENGTH,
    RESERVED_FRAME_FLAGS_MASK, RPC_BATCH_FRAME_CODE, RPC_COMPLETION_FRAME_CODE,
    RPC_EXECUTE_REQUEST_FRAME_CODE, RPC_METADATA_FRAME_CODE, TELEMETRY_SOFT_SIGNAL_FRAME_CODE,
};

pub use struct_::{FrameBytes, FrameHeader};

pub use codec::{FrameCodec, FrameCodecEndian, FRAME_CODEC_HEADER_LEN, FRAME_CODEC_VERSION};

pub use sequence::{
    validate_result_stream_sequence, validate_result_stream_sequence_with_metadata_policy,
    ResultStreamMetadataPolicy, ResultStreamSequence,
};

pub use super::validate_frame_sequence_impl;
pub use super::validate_single_frame_on_stream_impl;
