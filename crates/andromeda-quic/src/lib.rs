#![forbid(unsafe_code)]

//! # Andromeda QUIC Transport Layer
//!
//! This crate provides the QUIC transport layer for Andromeda, implementing frame encoding,
//! stream management, flow control, connection lifecycle, and RPC dispatch.
//!
//! ## Architecture
//!
//! The transport layer is organized into four protocol layers:
//!
//! - **Frame Layer** (`frame`): Frame type codes, encoding, and decoding
//! - **Stream Layer** (`stream`): Stream state machines and flow control
//! - **Connection Layer** (`connection`): Session management and connection lifecycle
//! - **RPC Layer** (`rpc`): Stream-to-RPC mapping and dispatch
//!
//! ## Key Invariants
//!
//! - Frame type codes are locked to protobuf payload layer codes
//! - Stream roles enforce transport surface separation (bidirectional, unidirectional, datagram)
//! - Result streams follow strict sequencing: metadata → batch* → completion
//! - Flow control windows prevent buffer saturation and enable backpressure signaling
//! - Connection lifecycle enforces handshake, active, and close states

// ============================================================================
// Module files for frame layer
// ============================================================================

mod backpressure;
mod connection;
mod frame_code;
mod frame_codec;
mod frame_sequence;
mod frame_struct;
mod procedure_gateway;
mod rpc_dispatch;
mod stream_types;

pub mod frame;
pub mod stream_concurrency;
pub mod transport;

#[cfg(feature = "runtime-quinn")]
mod runtime_quinn;

#[cfg(feature = "runtime-quinn")]
pub mod quinn_backend;

#[cfg(feature = "runtime-quinn")]
pub mod quinn_tls;

pub use stream_concurrency::{
    BackpressureRequest, CancellationReason, CancellationToken, StreamConcurrencyManager,
    StreamState,
};

// ============================================================================
// Identity Extraction (D3)
// ============================================================================

pub mod mtls_identity;

pub use frame::{
    AUTH_FRAME_CODE, CONTRACT_REQUEST_FRAME_CODE, CONTRACT_RESPONSE_FRAME_CODE, ERROR_FRAME_CODE,
    FRAME_CODEC_CRC_OFFSET, FRAME_CODEC_HEADER_LEN, FRAME_HEADER_CRC_UNCHECKED,
    FRAME_TYPE_PAYLOAD_CODE_LOCKSTEP, FrameBytes, FrameCodec, FrameCodecEndian, FrameFamily,
    FrameHeader, FrameType, HELLO_FRAME_CODE, MAX_FRAME_PAYLOAD_LENGTH, RESERVED_FRAME_FLAGS_MASK,
    RPC_BATCH_FRAME_CODE, RPC_COMPLETION_FRAME_CODE, RPC_EXECUTE_REQUEST_FRAME_CODE,
    RPC_METADATA_FRAME_CODE, ResultStreamMetadataPolicy, ResultStreamSequence, StreamRole,
    TELEMETRY_SOFT_SIGNAL_FRAME_CODE, validate_frame_sequence, validate_result_stream_sequence,
    validate_result_stream_sequence_with_metadata_policy, validate_single_frame_on_stream,
};

// ============================================================================
// Stream Protocol Layer
// ============================================================================

pub mod stream;

// ============================================================================
// Connection Protocol Layer
// ============================================================================

pub mod session;

pub use session::{
    CancellationCause, CancellationOutcome, CancellationSignal, Connection, DatagramPolicy,
    EarlyDataPolicy, LifecycleState, SurfaceListenerConfig, SurfaceListenerSet, SurfacePlane,
};

// ============================================================================
// RPC Dispatch Layer
// ============================================================================

pub mod rpc;

pub use rpc::{
    DispatchPolicy, FrameDispatch, TransportSurface, dispatch_frame, expected_stream_role,
    validate_transport_surface,
};

// ============================================================================
// Procedure Gateway
// ============================================================================

pub use procedure_gateway::ProcedureGateway;

// ============================================================================
// Backpressure
// ============================================================================

pub use backpressure::{BackpressureReason, BackpressureSignal, BackpressureTransport};

// ============================================================================
// Runtime-free transport trait boundary
// ============================================================================

pub use transport::{
    QuicClientTransport, QuicServerTransport, TransportBackpressureStatus,
    TransportCancellationStatus, TransportEndpointMetadata, TransportMessage,
    TransportShutdownMode, TransportShutdownState,
};

// ============================================================================
// HA/DR Stream Mapping (F2)
// ============================================================================

pub mod hadr_streams;

pub use hadr_streams::{
    HADR_STREAM_MAX, HADR_STREAM_MIN, HEARTBEAT_STREAM_MAX, HEARTBEAT_STREAM_MIN,
    HadrStreamCleanup, HadrStreamKind, RESERVED_STREAM_MAX, RESERVED_STREAM_MIN, StreamAllocation,
    StreamMultiplexer, VOTE_STREAM_MAX, VOTE_STREAM_MIN, WAL_SHIPPING_STREAM_MAX,
    WAL_SHIPPING_STREAM_MIN,
};

// ============================================================================
// Protocol Invariants (D7)
// ============================================================================

pub mod protocol_invariants;

pub use protocol_invariants::{
    FrameTypeInvariants, PayloadKindInvariants, ProtocolInvariants, ProtocolVersionInvariants,
    validate_frame_header_layout,
};
