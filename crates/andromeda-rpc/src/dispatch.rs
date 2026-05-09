//! RPC stream-to-dispatch mapping.
//!
//! This module handles RPC dispatching and stream surface routing.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_rpc_codec::{TypedResultStreamBounds, TypedResultStreamContext};
use andromeda_rpc_protocol::{
    FrameBytes, FrameType, ResultStreamMetadataPolicy, ResultStreamSequence, StreamRole,
};

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
    result_context: Option<TypedResultStreamContext>,
    result_bounds: TypedResultStreamBounds,
    result_frame_count: usize,
    result_envelope_payload_bytes: u64,
}

impl DispatchPolicy {
    /// Creates a dispatch policy with default metadata policy.
    ///
    /// For `ResultUnidirectional`, this is a compatibility constructor that
    /// creates a fail-closed policy. Use [`Self::new_result_stream`] or
    /// [`Self::new_result_stream_with_context_and_bounds`] to admit a typed
    /// ResultStream with a route-bound [`TypedResultStreamContext`].
    pub const fn new(stream_role: StreamRole) -> Self {
        Self::new_legacy_fail_closed(stream_role)
    }

    /// Creates a dispatch policy with specified metadata policy.
    ///
    /// For `ResultUnidirectional`, this is a compatibility constructor that
    /// creates a fail-closed policy. Use
    /// [`Self::new_result_stream_with_metadata_policy`] to create a policy that
    /// can actually accept typed ResultStream frames.
    pub const fn new_with_result_metadata_policy(
        stream_role: StreamRole,
        metadata_policy: ResultStreamMetadataPolicy,
    ) -> Self {
        Self::new_legacy_fail_closed_with_result_metadata_policy(stream_role, metadata_policy)
    }

    /// Compatibility constructor for legacy call sites.
    ///
    /// A result-stream policy created here intentionally has no admitted route
    /// context and therefore fails closed on the first result frame or finish.
    pub const fn new_legacy_fail_closed(stream_role: StreamRole) -> Self {
        Self::new_legacy_fail_closed_with_result_metadata_policy(
            stream_role,
            ResultStreamMetadataPolicy::RowBatchRequired,
        )
    }

    /// Compatibility constructor for legacy call sites that need a metadata policy.
    ///
    /// A result-stream policy created here intentionally has no admitted route
    /// context and therefore fails closed on the first result frame or finish.
    pub const fn new_legacy_fail_closed_with_result_metadata_policy(
        stream_role: StreamRole,
        metadata_policy: ResultStreamMetadataPolicy,
    ) -> Self {
        Self {
            stream_role,
            result_sequence: ResultStreamSequence::new_with_metadata_policy(metadata_policy),
            result_context: None,
            result_bounds: TypedResultStreamBounds::v0_default(),
            result_frame_count: 0,
            result_envelope_payload_bytes: 0,
        }
    }

    /// Creates a result-stream dispatch policy from an admitted Procedure route context.
    pub const fn new_result_stream(result_context: TypedResultStreamContext) -> Self {
        Self::new_result_stream_with_metadata_policy(
            result_context,
            ResultStreamMetadataPolicy::RowBatchRequired,
        )
    }

    /// Creates a result-stream dispatch policy from an admitted Procedure route
    /// context and a ResultStream metadata policy.
    pub const fn new_result_stream_with_metadata_policy(
        result_context: TypedResultStreamContext,
        metadata_policy: ResultStreamMetadataPolicy,
    ) -> Self {
        Self::new_result_stream_with_context_and_bounds(
            result_context,
            metadata_policy,
            TypedResultStreamBounds::v0_default(),
        )
    }

    /// Creates a bounded result-stream dispatch policy from an admitted
    /// Procedure route context.
    pub const fn new_result_stream_with_context_and_bounds(
        result_context: TypedResultStreamContext,
        metadata_policy: ResultStreamMetadataPolicy,
        result_bounds: TypedResultStreamBounds,
    ) -> Self {
        Self {
            stream_role: StreamRole::ResultUnidirectional,
            result_sequence: ResultStreamSequence::new_with_metadata_policy(metadata_policy),
            result_context: Some(result_context),
            result_bounds,
            result_frame_count: 0,
            result_envelope_payload_bytes: 0,
        }
    }

    /// Returns the stream role.
    pub fn stream_role(&self) -> StreamRole {
        self.stream_role
    }

    /// Returns the admitted typed ResultStream context, if this is a result policy.
    pub const fn typed_result_stream_context(&self) -> Option<TypedResultStreamContext> {
        self.result_context
    }

    /// Dispatches a frame and updates state.
    pub fn dispatch(&mut self, frame: &FrameBytes) -> AndromedaResult<FrameDispatch> {
        let dispatch = dispatch_frame(frame, self.stream_role)?;

        if self.stream_role == StreamRole::ResultUnidirectional {
            self.dispatch_result_frame(frame)?;
        }

        Ok(dispatch)
    }

    /// Finalizes the policy and validates completion state.
    pub fn finish(self) -> AndromedaResult<()> {
        if self.stream_role == StreamRole::ResultUnidirectional {
            if self.result_context.is_none() {
                return Err(protocol_error(
                    "result stream dispatch requires admitted typed ResultStream context",
                ));
            }

            if !self.result_sequence.is_complete() {
                return Err(protocol_error("result stream ended before RPC completion"));
            }
        }

        Ok(())
    }

    fn dispatch_result_frame(&mut self, frame: &FrameBytes) -> AndromedaResult<()> {
        let Some(result_context) = self.result_context else {
            return Err(protocol_error(
                "result stream dispatch requires admitted typed ResultStream context",
            ));
        };

        self.result_bounds.validate()?;
        if self.result_frame_count >= self.result_bounds.max_frame_count {
            return Err(resource_error(
                "typed result-stream frame count exceeds bounded policy",
            ));
        }

        let next_payload_bytes = self
            .result_envelope_payload_bytes
            .checked_add(frame.payload.len() as u64)
            .ok_or_else(|| resource_error("typed result-stream byte budget overflow"))?;
        if next_payload_bytes > self.result_bounds.max_envelope_payload_bytes {
            return Err(resource_error(
                "typed result-stream envelope bytes exceed bounded policy",
            ));
        }

        result_context.validate_frame_envelope(frame)?;
        self.result_sequence.accept(frame)?;
        self.result_frame_count += 1;
        self.result_envelope_payload_bytes = next_payload_bytes;

        Ok(())
    }
}

/// Dispatches a frame on a stream.
pub fn dispatch_frame(
    frame: &FrameBytes,
    stream_role: StreamRole,
) -> AndromedaResult<FrameDispatch> {
    andromeda_rpc_protocol::frame::validate_single_frame_on_stream(frame, stream_role)?;

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
        },
        TransportSurface::Datagram => {
            if !frame.header.frame_type.allows_datagram() {
                return Err(protocol_error(
                    "only telemetry soft-signal frames may use QUIC DATAGRAM",
                ));
            }

            dispatch_frame(frame, StreamRole::TelemetryDatagram)
        },
    }
}

fn protocol_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}

fn resource_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Resource, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_proto_wire::{
        GeneratedFrameEnvelope, GeneratedPayloadKind, GeneratedProtocolVersion, GeneratedRpcBatch,
        GeneratedRpcCompletion, GeneratedRpcCompletionStatus, GeneratedRpcMetadata,
        GeneratedTransactionOutcome, encode_protobuf_message,
    };
    use andromeda_types::{CatalogVersion, ContractHash, RequestId, SessionId};

    use andromeda_rpc_protocol::{FRAME_HEADER_CRC_UNCHECKED, FrameHeader};

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

    fn hash(byte: u8) -> Vec<u8> {
        vec![byte; ContractHash::LEN]
    }

    fn result_context() -> TypedResultStreamContext {
        TypedResultStreamContext::new(
            RequestId::new(1),
            SessionId::new(2),
            None,
            ContractHash::from_slice(&hash(7)).unwrap(),
            CatalogVersion::new(42),
        )
    }

    fn envelope_payload(kind: GeneratedPayloadKind, payload: Vec<u8>) -> Vec<u8> {
        encode_protobuf_message(&GeneratedFrameEnvelope {
            protocol_version: Some(GeneratedProtocolVersion { major: 1, minor: 0 }),
            contract_hash: hash(7),
            catalog_version: 42,
            request_id: 1,
            session_id: 2,
            tx_id: None,
            payload_kind: kind as i32,
            payload,
        })
    }

    fn result_metadata_frame() -> FrameBytes {
        frame(
            FrameType::RpcMetadata,
            envelope_payload(
                GeneratedPayloadKind::RpcMetadata,
                encode_protobuf_message(&GeneratedRpcMetadata {
                    result_streams: Vec::new(),
                    completion_policy: None,
                }),
            ),
        )
    }

    fn result_batch_frame() -> FrameBytes {
        frame(
            FrameType::RpcBatch,
            envelope_payload(
                GeneratedPayloadKind::RpcBatch,
                encode_protobuf_message(&GeneratedRpcBatch {
                    result_name: "Inventory.ReserveStock.Reservation".to_string(),
                    batch_index: 0,
                    rows_emitted: 1,
                    structured_payload: b"\x01".to_vec(),
                    row_count_exact: Some(1),
                    terminal_batch: true,
                }),
            ),
        )
    }

    fn result_completion_frame() -> FrameBytes {
        frame(
            FrameType::RpcCompletion,
            envelope_payload(
                GeneratedPayloadKind::RpcCompletion,
                encode_protobuf_message(&GeneratedRpcCompletion {
                    status: GeneratedRpcCompletionStatus::Committed as i32,
                    rows_affected: Some(1),
                    tx_id: None,
                    request_id: Some(1),
                    session_id: Some(2),
                    trace_id: Some("trace".to_string()),
                    transaction_outcome: GeneratedTransactionOutcome::Committed as i32,
                    durable_lsn: Some(1),
                    result_row_counts: Vec::new(),
                }),
            ),
        )
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
        let mut missing_context = DispatchPolicy::new(StreamRole::ResultUnidirectional);
        assert_eq!(
            missing_context
                .dispatch(&result_metadata_frame())
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Protocol
        );

        let mut policy = DispatchPolicy::new_result_stream(result_context());

        policy.dispatch(&result_metadata_frame()).unwrap();
        assert_eq!(
            policy.finish().unwrap_err().kind(),
            AndromedaErrorKind::Protocol
        );

        let mut policy = DispatchPolicy::new_result_stream(result_context());
        policy.dispatch(&result_metadata_frame()).unwrap();
        policy.dispatch(&result_batch_frame()).unwrap();
        policy.dispatch(&result_completion_frame()).unwrap();
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
