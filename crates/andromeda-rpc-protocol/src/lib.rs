#![forbid(unsafe_code)]

//! Runtime-free Andromeda RPC protocol contracts.
//!
//! This crate owns the explicit frame wire contract, frame/stream routing
//! families, and ResultStream frame sequence rules. It deliberately does not
//! own QUIC sockets, Quinn, TLS, listener lifecycle, executor dispatch, IAM, WAL,
//! storage, or recovery behavior.

mod frame_code;
mod frame_codec;
mod frame_sequence;
mod frame_struct;
mod stream_types;

pub mod frame;
pub mod stream;

pub use frame::{
    AUTH_FRAME_CODE, CONTRACT_REQUEST_FRAME_CODE, CONTRACT_RESPONSE_FRAME_CODE, ERROR_FRAME_CODE,
    FRAME_CODEC_CRC_OFFSET, FRAME_CODEC_HEADER_LEN, FRAME_CODEC_VERSION,
    FRAME_HEADER_CRC_UNCHECKED, FRAME_TYPE_PAYLOAD_CODE_LOCKSTEP, FrameBytes, FrameCodec,
    FrameCodecEndian, FrameHeader, FrameType, HELLO_FRAME_CODE, MAX_FRAME_PAYLOAD_LENGTH,
    RESERVED_FRAME_FLAGS_MASK, RPC_BATCH_FRAME_CODE, RPC_COMPLETION_FRAME_CODE,
    RPC_EXECUTE_REQUEST_FRAME_CODE, RPC_METADATA_FRAME_CODE, ResultStreamMetadataPolicy,
    ResultStreamSequence, TELEMETRY_SOFT_SIGNAL_FRAME_CODE, validate_frame_sequence,
    validate_result_stream_sequence, validate_result_stream_sequence_with_metadata_policy,
    validate_single_frame_on_stream,
};
pub use stream::{FrameFamily, StreamRole};
