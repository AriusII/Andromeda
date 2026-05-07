//! Protocol stability contract tests.
//!
//! Validates that protocol invariants remain stable across commits.
//! These tests detect regressions in frame format, Protobuf schema,
//! and RPC contract discriminators.

use andromeda_core::{AndromedaErrorKind, RequestId, SessionId, TransactionId};
use andromeda_proto::{
    BackpressureMetadata, ErrorEnvelope, ErrorFamily, PayloadKind, ProtocolVersion,
    RetryDisposition, TransactionEffect,
};
use andromeda_quic::{
    AUTH_FRAME_CODE, BackpressureReason, BackpressureSignal, CONTRACT_REQUEST_FRAME_CODE,
    CONTRACT_RESPONSE_FRAME_CODE, ERROR_FRAME_CODE, FRAME_CODEC_CRC_OFFSET, FRAME_CODEC_HEADER_LEN,
    FrameCodec, FrameHeader, FrameType, FrameTypeInvariants, HELLO_FRAME_CODE,
    MAX_FRAME_PAYLOAD_LENGTH, PayloadKindInvariants, ProtocolInvariants, ProtocolVersionInvariants,
    RPC_BATCH_FRAME_CODE, RPC_COMPLETION_FRAME_CODE, RPC_EXECUTE_REQUEST_FRAME_CODE,
    RPC_METADATA_FRAME_CODE, TELEMETRY_SOFT_SIGNAL_FRAME_CODE, validate_frame_header_layout,
};
use std::mem;

#[path = "protocol_stability_contract/diagnostic_backpressure_metadata.rs"]
mod diagnostic_backpressure_metadata;
#[path = "protocol_stability_contract/forbidden_surface_drift.rs"]
mod forbidden_surface_drift;
#[path = "protocol_stability_contract/frame_header_layout.rs"]
mod frame_header_layout;
#[path = "protocol_stability_contract/frame_wire_lock.rs"]
mod frame_wire_lock;
#[path = "protocol_stability_contract/payload_frame_lockstep.rs"]
mod payload_frame_lockstep;
#[path = "protocol_stability_contract/protobuf_schema_lock.rs"]
mod protobuf_schema_lock;
