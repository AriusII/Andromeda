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

use andromeda_core::AndromedaResult;

// ============================================================================
// Module files for frame layer
// ============================================================================

mod backpressure;
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
        validate_result_stream_sequence, validate_result_stream_sequence_with_metadata_policy,
        ResultStreamMetadataPolicy, ResultStreamSequence,
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
    validate_result_stream_sequence, validate_result_stream_sequence_with_metadata_policy,
    validate_single_frame_on_stream, FrameBytes, FrameCodec, FrameCodecEndian, FrameFamily,
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

pub mod connection {
    //! Connection protocol layer: session management and lifecycle.

    /// Placeholder for connection lifecycle management.
    pub struct Connection {
        // Future: session state, handshake state, flow control windows
    }

    impl Connection {
        /// Creates a new connection.
        pub fn new() -> Self {
            Self {}
        }
    }

    impl Default for Connection {
        fn default() -> Self {
            Self::new()
        }
    }
}

pub use connection::Connection;

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

pub use backpressure::{BackpressureReason, BackpressureSignal};

// ============================================================================
// Frame Validation Helpers
// ============================================================================

/// Validates frame sequences based on stream role.
pub fn validate_frame_sequence(
    frames: &[FrameBytes],
    stream_role: StreamRole,
) -> AndromedaResult<()> {
    match stream_role {
        StreamRole::ResultUnidirectional => frame::validate_result_stream_sequence(frames),
        _ => {
            for frame in frames {
                frame::validate_single_frame_on_stream(frame, stream_role)?;
            }

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_type_codes_are_locked() {
        assert_eq!(HELLO_FRAME_CODE, 1);
        assert_eq!(AUTH_FRAME_CODE, 2);
        assert_eq!(RPC_EXECUTE_REQUEST_FRAME_CODE, 5);
        assert_eq!(RPC_BATCH_FRAME_CODE, 7);
        assert_eq!(TELEMETRY_SOFT_SIGNAL_FRAME_CODE, 100);
    }

    #[test]
    fn max_frame_payload_is_16_mib() {
        assert_eq!(MAX_FRAME_PAYLOAD_LENGTH, 16 * 1024 * 1024);
    }
}
