#![forbid(unsafe_code)]

//! Runtime-free RPC codec primitives.

mod protobuf;
mod result_metadata;
pub mod typed_envelope;

pub use protobuf::{decode_protobuf_message, encode_protobuf_message};
pub use result_metadata::{
    StructuredPayloadByteTracker, validate_result_batch_payload_parts,
    validate_rpc_execute_argument_value, validate_rpc_execute_arguments_len,
    validate_rpc_response_sequence_len,
};
pub use typed_envelope::{
    DEFAULT_MAX_TYPED_RESULT_STREAM_ENVELOPE_BYTES, DEFAULT_MAX_TYPED_RESULT_STREAM_FRAMES,
    TypedResultStreamBounds, TypedResultStreamContext, decode_typed_frame_envelope,
    validate_typed_result_stream_sequence,
    validate_typed_result_stream_sequence_with_context_and_bounds,
    validate_typed_result_stream_sequence_with_metadata_policy,
};
