#![forbid(unsafe_code)]

//! Andromeda QUIC transport contracts.
//!
//! The default API is runtime-free: frame encoding, stream role validation,
//! lifecycle gating, backpressure, RPC dispatch, and HA/DR stream allocation are
//! modelled without exposing a concrete QUIC backend. Quinn-backed TLS and
//! network adapters live in the separate `andromeda-quic-runtime-quinn` crate.
//!
//! Critical invariants:
//! - frame type codes stay locked to protobuf payload layer codes;
//! - stream roles enforce surface separation;
//! - result streams are ordered as metadata, zero or more batches, completion;
//! - lifecycle state gates handshake, active dispatch, drain, and close.

mod catalog_manifest_resolution;
mod connection;
mod procedure_gateway;
mod reconnect;
mod zero_rtt;

mod stream_concurrency;
mod transport;

pub use stream_concurrency::{
    BackpressureRequest, CancellationReason, CancellationToken, StreamConcurrencyManager,
    StreamState,
};

pub mod mtls_identity;

pub use andromeda_rpc_protocol::frame::{
    AUTH_FRAME_CODE, CONTRACT_REQUEST_FRAME_CODE, CONTRACT_RESPONSE_FRAME_CODE, ERROR_FRAME_CODE,
    FRAME_CODEC_CRC_OFFSET, FRAME_CODEC_HEADER_LEN, FRAME_HEADER_CRC_UNCHECKED,
    FRAME_TYPE_PAYLOAD_CODE_LOCKSTEP, FrameBytes, FrameCodec, FrameCodecEndian, FrameFamily,
    FrameHeader, FrameType, HELLO_FRAME_CODE, MAX_FRAME_PAYLOAD_LENGTH, RESERVED_FRAME_FLAGS_MASK,
    RPC_BATCH_FRAME_CODE, RPC_COMPLETION_FRAME_CODE, RPC_EXECUTE_REQUEST_FRAME_CODE,
    RPC_METADATA_FRAME_CODE, ResultStreamMetadataPolicy, ResultStreamSequence, StreamRole,
    TELEMETRY_SOFT_SIGNAL_FRAME_CODE, validate_frame_sequence, validate_result_stream_sequence,
    validate_result_stream_sequence_with_metadata_policy, validate_single_frame_on_stream,
};

mod session;

pub use session::{
    CancellationCause, CancellationOutcome, CancellationSignal, Connection, DatagramPolicy,
    EarlyDataPolicy, LifecycleState, SurfaceListenerConfig, SurfaceListenerSet, SurfacePlane,
};

pub use andromeda_rpc::{
    DispatchPolicy, FrameDispatch, TransportSurface, dispatch_frame, expected_stream_role,
    validate_transport_surface,
};

pub use andromeda_rpc_codec::{
    DEFAULT_MAX_TYPED_RESULT_STREAM_ENVELOPE_BYTES, DEFAULT_MAX_TYPED_RESULT_STREAM_FRAMES,
    TypedResultStreamBounds, TypedResultStreamContext, decode_typed_frame_envelope,
    validate_typed_result_stream_sequence,
    validate_typed_result_stream_sequence_with_context_and_bounds,
    validate_typed_result_stream_sequence_with_metadata_policy,
};

pub use procedure_gateway::{
    ProcedureAuthorizedRouteBinding, ProcedureGateway, ProcedureRouteAdmissionError,
    ProcedureRouteBinding,
};

pub use reconnect::{
    CertificateContinuityDecision, CertificateContinuityPolicy, CertificateRotationDeclaration,
    ConnectionPool, ConnectionPoolKey, ConnectionPoolPolicy, MAX_POOL_CONNECTIONS_PER_KEY,
    MAX_POOL_IDLE_TIMEOUT_MS, PoolAdmission, PoolAdmissionKind, PoolConnectionId, PooledConnection,
    PooledConnectionHealth, ReconnectAttemptTrace, ReconnectDecision, ReconnectPolicy,
    ReconnectState, RetryAdmissionDecision, RetryAdmissionPolicy, RetryIdempotency,
    RetryRejectionReason,
};

pub use andromeda_rpc_protocol::backpressure::{
    BackpressureReason, BackpressureSignal, BackpressureTransport,
};

pub use catalog_manifest_resolution::{
    CatalogManifestResolutionContext, CatalogManifestResolutionGateway,
    CatalogManifestResolutionRuntime,
};

pub use transport::{
    QuicClientTransport, QuicServerTransport, TransportBackpressureStatus,
    TransportCancellationStatus, TransportEndpointMetadata, TransportMessage,
    TransportShutdownMode, TransportShutdownState,
};

pub use andromeda_rpc_protocol::{
    FrameTypeInvariants, PayloadKindInvariants, ProtocolInvariants, ProtocolVersionInvariants,
    validate_frame_header_layout,
};

pub use zero_rtt::{
    ZeroRttAdmissionDecision, ZeroRttAdmissionPolicy, ZeroRttAdmissionRejectionReason,
    ZeroRttReplayClass,
};
