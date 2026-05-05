//! Frame protocol layer: types, codes, encoding, and decoding.

use andromeda_core::AndromedaResult;

pub use crate::frame_code::{
    AUTH_FRAME_CODE, CONTRACT_REQUEST_FRAME_CODE, CONTRACT_RESPONSE_FRAME_CODE, ERROR_FRAME_CODE,
    FRAME_HEADER_CRC_UNCHECKED, FRAME_TYPE_PAYLOAD_CODE_LOCKSTEP, FrameType, HELLO_FRAME_CODE,
    MAX_FRAME_PAYLOAD_LENGTH, RESERVED_FRAME_FLAGS_MASK, RPC_BATCH_FRAME_CODE,
    RPC_COMPLETION_FRAME_CODE, RPC_EXECUTE_REQUEST_FRAME_CODE, RPC_METADATA_FRAME_CODE,
    TELEMETRY_SOFT_SIGNAL_FRAME_CODE,
};

pub use crate::frame_codec::FRAME_CODEC_CRC_OFFSET;

pub use crate::stream_types::{FrameFamily, StreamRole};

pub use crate::frame_struct::{FrameBytes, FrameHeader};

pub use crate::frame_codec::{
    FRAME_CODEC_HEADER_LEN, FRAME_CODEC_VERSION, FrameCodec, FrameCodecEndian,
};

pub use crate::frame_sequence::{
    ResultStreamMetadataPolicy, ResultStreamSequence, validate_frame_sequence,
    validate_result_stream_sequence, validate_result_stream_sequence_with_metadata_policy,
};

/// Validates a single frame on a stream.
pub fn validate_single_frame_on_stream(
    frame: &FrameBytes,
    stream_role: StreamRole,
) -> AndromedaResult<()> {
    frame.validate(stream_role)
}
