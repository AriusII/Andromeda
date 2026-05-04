//! RPC stream-to-dispatch mapping.
//!
//! This module handles RPC dispatching and stream surface routing.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{FrameBytes, FrameType, ResultStreamMetadataPolicy, ResultStreamSequence, StreamRole};

/// Transport surface variant (reliable stream or datagram).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportSurface {
    /// Reliable QUIC stream.
    ReliableStream(StreamRole),
    /// QUIC datagram.
    Datagram,
}

/// Result of frame dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameDispatch {
    /// Stream role the frame was dispatched on.
    pub stream_role: StreamRole,
    /// Frame type.
    pub frame_type: FrameType,
}

/// RPC dispatch policy for a stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchPolicy {
    stream_role: StreamRole,
    result_sequence: ResultStreamSequence,
}

impl DispatchPolicy {
    /// Creates a new dispatch policy with default metadata policy.
    pub const fn new(stream_role: StreamRole) -> Self {
        Self::new_with_result_metadata_policy(
            stream_role,
            ResultStreamMetadataPolicy::RowBatchRequired,
        )
    }

    /// Creates a new dispatch policy with specified metadata policy.
    pub const fn new_with_result_metadata_policy(
        stream_role: StreamRole,
        metadata_policy: ResultStreamMetadataPolicy,
    ) -> Self {
        Self {
            stream_role,
            result_sequence: ResultStreamSequence::new_with_metadata_policy(metadata_policy),
        }
    }

    /// Returns the stream role.
    pub fn stream_role(&self) -> StreamRole {
        self.stream_role
    }

    /// Dispatches a frame and updates state.
    pub fn dispatch(&mut self, frame: &FrameBytes) -> AndromedaResult<FrameDispatch> {
        let dispatch = dispatch_frame(frame, self.stream_role)?;

        if self.stream_role == StreamRole::ResultUnidirectional {
            self.result_sequence.accept(frame)?;
        }

        Ok(dispatch)
    }

    /// Finalizes the policy and validates completion state.
    pub fn finish(self) -> AndromedaResult<()> {
        if self.stream_role == StreamRole::ResultUnidirectional
            && !self.result_sequence.is_complete()
        {
            return Err(protocol_error("result stream ended before RPC completion"));
        }

        Ok(())
    }
}

/// Dispatches a frame on a stream.
pub fn dispatch_frame(
    frame: &FrameBytes,
    stream_role: StreamRole,
) -> AndromedaResult<FrameDispatch> {
    crate::frame::validate_single_frame_on_stream(frame, stream_role)?;

    Ok(FrameDispatch {
        stream_role,
        frame_type: frame.header.frame_type,
    })
}

/// Returns the expected stream role for a frame type.
pub fn expected_stream_role(frame: &FrameBytes) -> StreamRole {
    frame.header.frame_type.stream_role()
}

/// Validates frame dispatch on a transport surface.
pub fn validate_transport_surface(
    frame: &FrameBytes,
    surface: TransportSurface,
) -> AndromedaResult<FrameDispatch> {
    match surface {
        TransportSurface::ReliableStream(stream_role) => {
            if frame.header.frame_type.allows_datagram() {
                return Err(protocol_error(
                    "telemetry soft-signal frames must use QUIC DATAGRAM",
                ));
            }

            dispatch_frame(frame, stream_role)
        }
        TransportSurface::Datagram => {
            if !frame.header.frame_type.allows_datagram() {
                return Err(protocol_error(
                    "only telemetry soft-signal frames may use QUIC DATAGRAM",
                ));
            }

            dispatch_frame(frame, StreamRole::TelemetryDatagram)
        }
    }
}

fn protocol_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::{RequestId, SessionId};

    use crate::frame::{FrameHeader, FRAME_HEADER_CRC_UNCHECKED};

    fn frame(frame_type: FrameType, payload: Vec<u8>) -> FrameBytes {
        FrameBytes {
            header: FrameHeader {
                frame_type,
                request_id: RequestId::new(1),
                session_id: SessionId::new(2),
                tx_id: None,
                payload_length: payload.len() as u64,
                flags: 0,
                header_crc: FRAME_HEADER_CRC_UNCHECKED,
            },
            payload,
        }
    }

    #[test]
    fn dispatcher_maps_frames_to_stream_roles() {
        let execute = frame(FrameType::RpcExecuteRequest, b"run".to_vec());

        assert_eq!(
            dispatch_frame(&execute, StreamRole::CommandBidirectional)
                .unwrap()
                .stream_role,
            StreamRole::CommandBidirectional
        );
        assert_eq!(
            expected_stream_role(&execute),
            StreamRole::CommandBidirectional
        );
        assert_eq!(
            dispatch_frame(&execute, StreamRole::ResultUnidirectional)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );
    }

    #[test]
    fn dispatcher_enforces_result_sequence_completion() {
        let mut policy = DispatchPolicy::new(StreamRole::ResultUnidirectional);

        policy
            .dispatch(&frame(FrameType::RpcMetadata, b"meta".to_vec()))
            .unwrap();
        assert_eq!(
            policy.finish().unwrap_err().kind(),
            AndromedaErrorKind::Protocol
        );

        let mut policy = DispatchPolicy::new(StreamRole::ResultUnidirectional);
        policy
            .dispatch(&frame(FrameType::RpcMetadata, b"meta".to_vec()))
            .unwrap();
        policy
            .dispatch(&frame(FrameType::RpcBatch, b"row".to_vec()))
            .unwrap();
        policy
            .dispatch(&frame(FrameType::RpcCompletion, Vec::new()))
            .unwrap();
        assert!(policy.finish().is_ok());
    }

    #[test]
    fn transport_surface_keeps_telemetry_datagram_only() {
        let telemetry = frame(FrameType::TelemetrySoftSignal, b"soft".to_vec());
        assert!(validate_transport_surface(&telemetry, TransportSurface::Datagram).is_ok());
        assert_eq!(
            validate_transport_surface(
                &telemetry,
                TransportSurface::ReliableStream(StreamRole::TelemetryDatagram),
            )
            .unwrap_err()
            .kind(),
            AndromedaErrorKind::Protocol
        );
    }
}
