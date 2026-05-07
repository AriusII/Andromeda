#![forbid(unsafe_code)]

//! Protobuf frame and result stream compatibility tests for Andromeda V0.5.
//!
//! Validates:
//! - Frame envelope round-trip serialization (deterministic)
//! - Result stream metadata preservation
//! - Row count exact encoding
//! - Completion signal variants
//! - Error envelope preservation
//! - Schema version tags in frames
//! - Malformed protobuf produces typed errors
//! - Partial read backpressure signals
//! - Multiple completion signals rejection

#[path = "support/proto_wire_fixtures.rs"]
pub(crate) mod proto_wire_fixtures;

#[path = "frame_result_compat/completion.rs"]
mod completion;
#[path = "frame_result_compat/error.rs"]
mod error;
#[path = "frame_result_compat/frame.rs"]
mod frame;
#[path = "frame_result_compat/result_stream.rs"]
mod result_stream;
