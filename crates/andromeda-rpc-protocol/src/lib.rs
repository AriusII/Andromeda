#![forbid(unsafe_code)]

//! Runtime-free Andromeda RPC protocol contracts.
//!
//! This crate owns the explicit frame wire contract, frame/stream routing
//! families, and ResultStream frame sequence rules. It deliberately does not
//! own QUIC sockets, Quinn, TLS, listener lifecycle, executor dispatch, IAM, WAL,
//! storage, or recovery behavior.

mod envelope;
mod errors;
mod frame_code;
mod frame_codec;
mod frame_sequence;
mod frame_struct;
mod protocol_invariants;
mod stream_types;
#[cfg(test)]
mod test_support;

pub mod backpressure;
pub mod frame;
pub mod stream;

pub use backpressure::{BackpressureReason, BackpressureSignal, BackpressureTransport};
pub use envelope::{
    AUTH_WIRE_CODE, COMPLETION_ENVELOPE_VERSION, CONTRACT_REQUEST_WIRE_CODE,
    CONTRACT_RESPONSE_WIRE_CODE, CompletionProtocolVersion, ERROR_WIRE_CODE, FrameEnvelope,
    HELLO_WIRE_CODE, PAYLOAD_KIND_TRANSPORT_CODE_LOCKSTEP, PayloadFrameFamily, PayloadFrameMapping,
    PayloadKind, ProtocolVersion, RPC_BATCH_WIRE_CODE, RPC_COMPLETION_WIRE_CODE,
    RPC_EXECUTE_REQUEST_WIRE_CODE, RPC_METADATA_WIRE_CODE, RpcResultStreamMetadataPolicy,
};
pub use errors::{
    BackpressureMetadata, ErrorEnvelope, ErrorFamily, RetryDisposition, TransactionEffect,
};
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
pub use protocol_invariants::{
    FrameTypeInvariants, PayloadKindInvariants, ProtocolInvariants, ProtocolVersionInvariants,
    validate_frame_header_layout,
};
pub use stream::{FrameFamily, StreamRole};
