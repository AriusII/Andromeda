#![forbid(unsafe_code)]

//! Andromeda QUIC transport contracts.
//!
//! The default API is runtime-free: frame encoding, stream role validation,
//! lifecycle gating, backpressure, RPC dispatch, and HA/DR stream allocation are
//! modelled without exposing a concrete QUIC backend. The optional
//! `runtime-quinn` feature adds Quinn-backed TLS and network adapters.
//!
//! Critical invariants:
//! - frame type codes stay locked to protobuf payload layer codes;
//! - stream roles enforce surface separation;
//! - result streams are ordered as metadata, zero or more batches, completion;
//! - lifecycle state gates handshake, active dispatch, drain, and close.

mod backpressure;
mod catalog_manifest_resolution;
mod connection;
mod frame_code;
mod frame_codec;
mod frame_sequence;
mod frame_struct;
mod procedure_gateway;
mod reconnect;
mod rpc_dispatch;
mod stream_types;
mod zero_rtt;

pub mod frame;
pub mod stream_concurrency;
mod transport;

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

pub mod stream;

mod session;

pub use session::{
    CancellationCause, CancellationOutcome, CancellationSignal, Connection, DatagramPolicy,
    EarlyDataPolicy, LifecycleState, SurfaceListenerConfig, SurfaceListenerSet, SurfacePlane,
};

mod rpc;

pub use rpc::{
    DispatchPolicy, FrameDispatch, TransportSurface, dispatch_frame, expected_stream_role,
    validate_transport_surface,
};

pub use procedure_gateway::ProcedureGateway;

pub use reconnect::{
    CertificateContinuityDecision, CertificateContinuityPolicy, CertificateRotationDeclaration,
    ConnectionPool, ConnectionPoolKey, ConnectionPoolPolicy, MAX_POOL_CONNECTIONS_PER_KEY,
    MAX_POOL_IDLE_TIMEOUT_MS, PoolAdmission, PoolAdmissionKind, PoolConnectionId, PooledConnection,
    PooledConnectionHealth, ReconnectAttemptTrace, ReconnectDecision, ReconnectPolicy,
    ReconnectState, RetryAdmissionDecision, RetryAdmissionPolicy, RetryIdempotency,
    RetryRejectionReason,
};

pub use backpressure::{BackpressureReason, BackpressureSignal, BackpressureTransport};

pub use catalog_manifest_resolution::{
    CatalogColumnDescriptor, CatalogManifestResolutionContext, CatalogManifestResolutionGateway,
    CatalogManifestResolutionRequest, CatalogManifestResolutionResponse,
    CatalogManifestResolutionRuntime, CatalogManifestResolutionStatus, CatalogManifestSelector,
    CatalogProcedureManifest, CatalogProcedureManifestResolutionRequest,
    CatalogProcedureManifestResolutionResponse, CatalogProcedureProtocolLayout,
    CatalogRequiredPermission, CatalogResultStreamDescriptor,
    catalog_manifest_resolution_request_frame, decode_catalog_manifest_resolution_request_frame,
    decode_catalog_manifest_resolution_response_frame,
};

pub use transport::{
    QuicClientTransport, QuicServerTransport, TransportBackpressureStatus,
    TransportCancellationStatus, TransportEndpointMetadata, TransportMessage,
    TransportShutdownMode, TransportShutdownState,
};

pub mod hadr_streams;

pub use hadr_streams::{
    HADR_STREAM_MAX, HADR_STREAM_MIN, HEARTBEAT_STREAM_MAX, HEARTBEAT_STREAM_MIN,
    HadrStreamCleanup, HadrStreamKind, RESERVED_STREAM_MAX, RESERVED_STREAM_MIN, StreamAllocation,
    StreamMultiplexer, VOTE_STREAM_MAX, VOTE_STREAM_MIN, WAL_SHIPPING_STREAM_MAX,
    WAL_SHIPPING_STREAM_MIN,
};

mod protocol_invariants;

pub use protocol_invariants::{
    FrameTypeInvariants, PayloadKindInvariants, ProtocolInvariants, ProtocolVersionInvariants,
    validate_frame_header_layout,
};

pub use zero_rtt::{
    ZeroRttAdmissionDecision, ZeroRttAdmissionPolicy, ZeroRttAdmissionRejectionReason,
    ZeroRttReplayClass,
};
