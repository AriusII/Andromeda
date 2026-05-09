//! Protocol stability contract tests.
//!
//! Validates that protocol invariants remain stable across commits.
//! These tests detect regressions in frame format, Protobuf schema,
//! and RPC contract discriminators.

use andromeda_error::AndromedaErrorKind;
use andromeda_rpc_protocol::{
    AUTH_FRAME_CODE, BackpressureReason, BackpressureSignal, CONTRACT_REQUEST_FRAME_CODE,
    CONTRACT_RESPONSE_FRAME_CODE, ERROR_FRAME_CODE, FRAME_CODEC_CRC_OFFSET, FRAME_CODEC_HEADER_LEN,
    FrameCodec, FrameHeader, FrameType, FrameTypeInvariants, HELLO_FRAME_CODE,
    MAX_FRAME_PAYLOAD_LENGTH, PayloadKind, ProtocolInvariants, RPC_BATCH_FRAME_CODE,
    RPC_COMPLETION_FRAME_CODE, RPC_EXECUTE_REQUEST_FRAME_CODE, RPC_METADATA_FRAME_CODE,
    RetryDisposition, TELEMETRY_SOFT_SIGNAL_FRAME_CODE, TransactionEffect,
    validate_frame_header_layout,
};
use andromeda_rpc_protocol::{BackpressureMetadata, ErrorEnvelope, ErrorFamily};
use andromeda_types::{RequestId, SessionId, TransactionId};

#[path = "protocol_stability_contract/diagnostic_backpressure_metadata.rs"]
mod diagnostic_backpressure_metadata;
#[path = "protocol_stability_contract/frame_header_layout.rs"]
mod frame_header_layout;
#[path = "protocol_stability_contract/frame_wire_lock.rs"]
mod frame_wire_lock;
#[path = "protocol_stability_contract/payload_frame_lockstep.rs"]
mod payload_frame_lockstep;
