//! Runtime-free QUIC transport trait boundary.
//!
//! This module defines the client/server boundary used by Andromeda QUIC
//! integrations before a concrete socket backend exists.  The boundary carries
//! Andromeda custom QUIC RPC frames whose payload bytes are protobuf envelopes;
//! it deliberately does not expose concrete `quinn`, TLS, executor, or runtime
//! types.
//!
//! ## Contract
//!
//! * [`TransportMessage`] is the only message shape crossing this boundary.
//!   Its [`FrameBytes`] payload is treated as protobuf envelope bytes.
//! * Surface-plane and certificate identity metadata travel beside the frame so
//!   dispatch layers can authorize without depending on a network backend.
//! * Cancellation, backpressure, and shutdown are represented as typed trait
//!   methods instead of hidden runtime side channels.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, RequestId, SessionId};
use andromeda_observe::CertificateIdentity;

use crate::{BackpressureSignal, CancellationSignal, FrameBytes, StreamRole, SurfacePlane};

/// Peer/session metadata visible at the transport boundary.
///
/// Concrete listener/client implementations are expected to derive this from
/// handshake state and mTLS identity extraction, but the type itself remains
/// independent of any networking backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportEndpointMetadata {
    surface_plane: SurfacePlane,
    session_id: Option<SessionId>,
    certificate_identity: Option<CertificateIdentity>,
}

impl TransportEndpointMetadata {
    /// Creates endpoint metadata for a surface plane.
    pub fn new(
        surface_plane: SurfacePlane,
        session_id: Option<SessionId>,
        certificate_identity: Option<CertificateIdentity>,
    ) -> Self {
        Self {
            surface_plane,
            session_id,
            certificate_identity,
        }
    }

    /// Surface plane this endpoint/session is bound to.
    pub const fn surface_plane(&self) -> SurfacePlane {
        self.surface_plane
    }

    /// Negotiated session id, if the handshake has established one.
    pub const fn session_id(&self) -> Option<SessionId> {
        self.session_id
    }

    /// Certificate identity bound to this endpoint/session, if present.
    pub const fn certificate_identity(&self) -> Option<&CertificateIdentity> {
        self.certificate_identity.as_ref()
    }
}

/// A custom QUIC RPC frame plus transport metadata.
///
/// The `frame.payload` bytes are protobuf envelope bytes.  This type does not
/// know how to parse those bytes; protobuf contract ownership remains outside
/// the transport layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportMessage {
    metadata: TransportEndpointMetadata,
    stream_role: StreamRole,
    frame: FrameBytes,
}

impl TransportMessage {
    /// Builds and validates a transport-bound frame message.
    ///
    /// Validation is intentionally limited to transport invariants:
    /// stream-role admission, payload length, surface-plane family admission,
    /// and optional session-id consistency.
    pub fn new(
        metadata: TransportEndpointMetadata,
        stream_role: StreamRole,
        frame: FrameBytes,
    ) -> AndromedaResult<Self> {
        frame.validate(stream_role)?;

        if !metadata
            .surface_plane
            .permits_family(frame.header.frame_type.frame_family())
        {
            return Err(transport_protocol_error(
                "frame family not permitted on endpoint surface plane",
            ));
        }

        if let Some(session_id) = metadata.session_id
            && session_id != frame.header.session_id
        {
            return Err(transport_protocol_error(
                "frame session id does not match endpoint metadata",
            ));
        }

        Ok(Self {
            metadata,
            stream_role,
            frame,
        })
    }

    /// Metadata attached to this transport message.
    pub const fn metadata(&self) -> &TransportEndpointMetadata {
        &self.metadata
    }

    /// Stream role used for frame validation and dispatch.
    pub const fn stream_role(&self) -> StreamRole {
        self.stream_role
    }

    /// Custom QUIC RPC frame being exchanged.
    pub const fn frame(&self) -> &FrameBytes {
        &self.frame
    }

    /// Request correlation id copied from the frame header.
    pub const fn request_id(&self) -> RequestId {
        self.frame.header.request_id
    }

    /// Protobuf envelope bytes carried by this frame.
    pub fn protobuf_envelope_bytes(&self) -> &[u8] {
        &self.frame.payload
    }

    /// Consumes the message and returns the underlying frame.
    pub fn into_frame(self) -> FrameBytes {
        self.frame
    }
}

/// Result of accepting a typed cancellation request at the transport boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportCancellationStatus {
    /// Cancellation was accepted for routing to the owning invocation/stream.
    Accepted,
    /// The request or stream has already reached a terminal state.
    AlreadyTerminal,
    /// The request is unknown to this endpoint.
    UnknownRequest,
}

/// Backpressure status exposed by a transport endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportBackpressureStatus {
    /// Endpoint can currently accept more request frames.
    Ready,
    /// Endpoint is throttling and provides bounded retry guidance.
    Throttled(BackpressureSignal),
}

/// Shutdown mode requested at the transport boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportShutdownMode {
    /// Stop accepting new request frames while allowing in-flight frames to
    /// drain to completion.
    GracefulDrain,
    /// Close immediately; in-flight streams are expected to observe
    /// cancellation or connection loss.
    ImmediateClose,
}

/// Endpoint shutdown state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportShutdownState {
    /// Endpoint is open for new request frames.
    Open,
    /// Endpoint is draining and should reject new request frames.
    Draining,
    /// Endpoint is closed and no further frames should be exchanged.
    Closed,
}

/// Client-side transport trait for custom QUIC RPC frame exchange.
///
/// Implementations may be backed by a real QUIC stack, an in-memory harness, or
/// a deterministic simulator.  The trait intentionally exposes only Andromeda
/// transport types.
pub trait QuicClientTransport {
    /// Metadata for the local client/session endpoint.
    fn endpoint_metadata(&self) -> &TransportEndpointMetadata;

    /// Exchange one request frame for one response frame.
    fn exchange_frame(&mut self, request: TransportMessage) -> AndromedaResult<TransportMessage>;

    /// Submit a typed cancellation signal.
    fn cancel_request(
        &mut self,
        signal: CancellationSignal,
    ) -> AndromedaResult<TransportCancellationStatus>;

    /// Observe endpoint backpressure state.
    fn backpressure_status(&self) -> TransportBackpressureStatus;

    /// Request transport shutdown or drain.
    fn shutdown(&mut self, mode: TransportShutdownMode) -> AndromedaResult<TransportShutdownState>;
}

/// Server-side transport trait for custom QUIC RPC frame exchange.
///
/// This is the backend-neutral accept/dispatch boundary.  It does not define a
/// listener loop; concrete QUIC streams can adapt into this trait without
/// leaking backend types.
pub trait QuicServerTransport {
    /// Metadata for the server listener/session endpoint.
    fn endpoint_metadata(&self) -> &TransportEndpointMetadata;

    /// Handle one inbound request frame and produce one outbound response frame.
    fn handle_frame(&mut self, request: TransportMessage) -> AndromedaResult<TransportMessage>;

    /// Route a typed cancellation signal to the server-side stream owner.
    fn cancel_request(
        &mut self,
        signal: CancellationSignal,
    ) -> AndromedaResult<TransportCancellationStatus>;

    /// Observe server-side backpressure state.
    fn backpressure_status(&self) -> TransportBackpressureStatus;

    /// Request server-side drain or close.
    fn shutdown(&mut self, mode: TransportShutdownMode) -> AndromedaResult<TransportShutdownState>;
}

fn transport_protocol_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::{RequestId, SessionId};

    use crate::{CancellationCause, FRAME_HEADER_CRC_UNCHECKED, FrameHeader, FrameType};

    fn metadata() -> TransportEndpointMetadata {
        TransportEndpointMetadata::new(SurfacePlane::Application, Some(SessionId::new(7)), None)
    }

    fn frame(frame_type: FrameType, payload: Vec<u8>) -> FrameBytes {
        FrameBytes {
            header: FrameHeader {
                frame_type,
                request_id: RequestId::new(11),
                session_id: SessionId::new(7),
                tx_id: None,
                payload_length: payload.len() as u64,
                flags: 0,
                header_crc: FRAME_HEADER_CRC_UNCHECKED,
            },
            payload,
        }
    }

    #[derive(Debug)]
    struct EchoServer {
        metadata: TransportEndpointMetadata,
        shutdown_state: TransportShutdownState,
        cancelled: bool,
    }

    impl EchoServer {
        fn new() -> Self {
            Self {
                metadata: metadata(),
                shutdown_state: TransportShutdownState::Open,
                cancelled: false,
            }
        }
    }

    impl QuicServerTransport for EchoServer {
        fn endpoint_metadata(&self) -> &TransportEndpointMetadata {
            &self.metadata
        }

        fn handle_frame(&mut self, request: TransportMessage) -> AndromedaResult<TransportMessage> {
            if self.shutdown_state != TransportShutdownState::Open {
                return Err(transport_protocol_error(
                    "server transport is not accepting new frames",
                ));
            }

            assert_eq!(request.stream_role(), StreamRole::CommandBidirectional);
            let response = frame(
                FrameType::RpcBatch,
                request.protobuf_envelope_bytes().to_vec(),
            );
            TransportMessage::new(
                self.metadata.clone(),
                StreamRole::ResultUnidirectional,
                response,
            )
        }

        fn cancel_request(
            &mut self,
            _signal: CancellationSignal,
        ) -> AndromedaResult<TransportCancellationStatus> {
            self.cancelled = true;
            Ok(TransportCancellationStatus::Accepted)
        }

        fn backpressure_status(&self) -> TransportBackpressureStatus {
            TransportBackpressureStatus::Ready
        }

        fn shutdown(
            &mut self,
            mode: TransportShutdownMode,
        ) -> AndromedaResult<TransportShutdownState> {
            self.shutdown_state = match mode {
                TransportShutdownMode::GracefulDrain => TransportShutdownState::Draining,
                TransportShutdownMode::ImmediateClose => TransportShutdownState::Closed,
            };
            Ok(self.shutdown_state)
        }
    }

    struct InMemoryClient<S> {
        metadata: TransportEndpointMetadata,
        server: S,
        shutdown_state: TransportShutdownState,
    }

    impl<S> InMemoryClient<S> {
        fn new(server: S) -> Self {
            Self {
                metadata: metadata(),
                server,
                shutdown_state: TransportShutdownState::Open,
            }
        }
    }

    impl<S: QuicServerTransport> QuicClientTransport for InMemoryClient<S> {
        fn endpoint_metadata(&self) -> &TransportEndpointMetadata {
            &self.metadata
        }

        fn exchange_frame(
            &mut self,
            request: TransportMessage,
        ) -> AndromedaResult<TransportMessage> {
            if self.shutdown_state != TransportShutdownState::Open {
                return Err(transport_protocol_error(
                    "client transport is not accepting new frames",
                ));
            }
            self.server.handle_frame(request)
        }

        fn cancel_request(
            &mut self,
            signal: CancellationSignal,
        ) -> AndromedaResult<TransportCancellationStatus> {
            self.server.cancel_request(signal)
        }

        fn backpressure_status(&self) -> TransportBackpressureStatus {
            self.server.backpressure_status()
        }

        fn shutdown(
            &mut self,
            mode: TransportShutdownMode,
        ) -> AndromedaResult<TransportShutdownState> {
            self.shutdown_state = match mode {
                TransportShutdownMode::GracefulDrain => TransportShutdownState::Draining,
                TransportShutdownMode::ImmediateClose => TransportShutdownState::Closed,
            };
            Ok(self.shutdown_state)
        }
    }

    #[test]
    fn transport_message_carries_protobuf_envelope_bytes_and_metadata() {
        let message = TransportMessage::new(
            metadata(),
            StreamRole::CommandBidirectional,
            frame(FrameType::RpcExecuteRequest, b"protobuf-envelope".to_vec()),
        )
        .unwrap();

        assert_eq!(
            message.metadata().surface_plane(),
            SurfacePlane::Application
        );
        assert_eq!(message.metadata().session_id(), Some(SessionId::new(7)));
        assert_eq!(message.request_id(), RequestId::new(11));
        assert_eq!(message.protobuf_envelope_bytes(), b"protobuf-envelope");
    }

    #[test]
    fn transport_message_rejects_session_mismatch() {
        let mut bad = frame(FrameType::RpcExecuteRequest, b"request".to_vec());
        bad.header.session_id = SessionId::new(99);

        let err =
            TransportMessage::new(metadata(), StreamRole::CommandBidirectional, bad).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    }

    #[test]
    fn in_memory_client_server_proves_request_response_shape() {
        let server = EchoServer::new();
        let mut client = InMemoryClient::new(server);

        let request = TransportMessage::new(
            client.endpoint_metadata().clone(),
            StreamRole::CommandBidirectional,
            frame(FrameType::RpcExecuteRequest, b"execute-envelope".to_vec()),
        )
        .unwrap();

        let response = client.exchange_frame(request).unwrap();

        assert_eq!(response.stream_role(), StreamRole::ResultUnidirectional);
        assert_eq!(response.frame().header.frame_type, FrameType::RpcBatch);
        assert_eq!(response.protobuf_envelope_bytes(), b"execute-envelope");
        assert_eq!(
            client.backpressure_status(),
            TransportBackpressureStatus::Ready
        );
    }

    #[test]
    fn cancellation_and_shutdown_are_part_of_trait_boundary() {
        let server = EchoServer::new();
        let mut client = InMemoryClient::new(server);

        let cancellation = CancellationSignal {
            request_id: RequestId::new(11),
            session_id: SessionId::new(7),
            cause: CancellationCause::ClientRequested,
        };

        assert_eq!(
            client.cancel_request(cancellation).unwrap(),
            TransportCancellationStatus::Accepted
        );
        assert_eq!(
            client
                .shutdown(TransportShutdownMode::GracefulDrain)
                .unwrap(),
            TransportShutdownState::Draining
        );
    }
}
