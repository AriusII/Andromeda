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
mod rpc_dispatch;
mod stream_types;

pub mod frame {
    //! Frame protocol layer: types, codes, encoding, and decoding.

    use andromeda_core::AndromedaResult;

    pub use crate::frame_code::{
        FrameType, AUTH_FRAME_CODE, CONTRACT_REQUEST_FRAME_CODE, CONTRACT_RESPONSE_FRAME_CODE,
        ERROR_FRAME_CODE, FRAME_HEADER_CRC_UNCHECKED, FRAME_TYPE_PAYLOAD_CODE_LOCKSTEP,
        HELLO_FRAME_CODE, MAX_FRAME_PAYLOAD_LENGTH, RESERVED_FRAME_FLAGS_MASK,
        RPC_BATCH_FRAME_CODE, RPC_COMPLETION_FRAME_CODE, RPC_EXECUTE_REQUEST_FRAME_CODE,
        RPC_METADATA_FRAME_CODE, TELEMETRY_SOFT_SIGNAL_FRAME_CODE,
    };

    pub use crate::frame_codec::FRAME_CODEC_CRC_OFFSET;

    pub use crate::stream_types::{FrameFamily, StreamRole};

    pub use crate::frame_struct::{FrameBytes, FrameHeader};

    pub use crate::frame_codec::{
        FrameCodec, FrameCodecEndian, FRAME_CODEC_HEADER_LEN, FRAME_CODEC_VERSION,
    };

    pub use crate::frame_sequence::{
        validate_frame_sequence, validate_result_stream_sequence,
        validate_result_stream_sequence_with_metadata_policy, ResultStreamMetadataPolicy,
        ResultStreamSequence,
    };

    /// Validates a single frame on a stream.
    pub fn validate_single_frame_on_stream(
        frame: &FrameBytes,
        stream_role: StreamRole,
    ) -> AndromedaResult<()> {
        frame.validate(stream_role)
    }
}

pub use frame::{
    validate_frame_sequence, validate_result_stream_sequence,
    validate_result_stream_sequence_with_metadata_policy, validate_single_frame_on_stream,
    FrameBytes, FrameCodec, FrameCodecEndian, FrameFamily,
    FrameHeader, FrameType, ResultStreamMetadataPolicy, ResultStreamSequence, StreamRole,
    AUTH_FRAME_CODE, CONTRACT_REQUEST_FRAME_CODE, CONTRACT_RESPONSE_FRAME_CODE, ERROR_FRAME_CODE,
    FRAME_CODEC_CRC_OFFSET, FRAME_CODEC_HEADER_LEN, FRAME_HEADER_CRC_UNCHECKED,
    FRAME_TYPE_PAYLOAD_CODE_LOCKSTEP, HELLO_FRAME_CODE, MAX_FRAME_PAYLOAD_LENGTH,
    RESERVED_FRAME_FLAGS_MASK, RPC_BATCH_FRAME_CODE, RPC_COMPLETION_FRAME_CODE,
    RPC_EXECUTE_REQUEST_FRAME_CODE, RPC_METADATA_FRAME_CODE, TELEMETRY_SOFT_SIGNAL_FRAME_CODE,
};

// ============================================================================
// Stream Protocol Layer
// ============================================================================

pub mod stream {
    //! Stream protocol layer: roles, flow control, and byte ordering.
    //!
    //! Note: StreamRole and FrameFamily are re-exported through the frame module
    //! for backward compatibility and consistency.
}

// ============================================================================
// Connection Protocol Layer
// ============================================================================

pub mod session {
    //! Connection protocol layer: session lifecycle and surface-plane gating.
    //!
    //! Re-exports the canonical [`crate::connection`] state machine. The
    //! module is named `session` to avoid colliding with the private
    //! implementation file while keeping the public concept (a QUIC session)
    //! discoverable.

    pub use crate::connection::{
        CancellationCause, CancellationOutcome, CancellationSignal, Connection, DatagramPolicy,
        EarlyDataPolicy, LifecycleState, SurfaceListenerConfig, SurfaceListenerSet, SurfacePlane,
    };
}

pub use session::{
    CancellationCause, CancellationOutcome, CancellationSignal, Connection, DatagramPolicy,
    EarlyDataPolicy, LifecycleState, SurfaceListenerConfig, SurfaceListenerSet, SurfacePlane,
};

// ============================================================================
// RPC Dispatch Layer
// ============================================================================

pub mod rpc {
    //! RPC dispatch layer: stream-to-RPC mapping and dispatch.

    pub use crate::rpc_dispatch::{
        dispatch_frame, expected_stream_role, validate_transport_surface, DispatchPolicy,
        FrameDispatch, TransportSurface,
    };
}

pub use rpc::{
    dispatch_frame, expected_stream_role, validate_transport_surface, DispatchPolicy,
    FrameDispatch, TransportSurface,
};

// ============================================================================
// Backpressure
// ============================================================================

pub use backpressure::{BackpressureReason, BackpressureSignal, BackpressureTransport};

