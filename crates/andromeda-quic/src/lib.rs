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

mod session;

pub use session::{
    CancellationCause, CancellationOutcome, CancellationSignal, Connection, DatagramPolicy,
    EarlyDataPolicy, LifecycleState, SurfaceListenerConfig, SurfaceListenerSet, SurfacePlane,
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

pub use catalog_manifest_resolution::{
    CatalogManifestResolutionContext, CatalogManifestResolutionGateway,
    CatalogManifestResolutionRuntime,
};

pub use transport::{
    QuicClientTransport, QuicServerTransport, TransportBackpressureStatus,
    TransportCancellationStatus, TransportEndpointMetadata, TransportMessage,
    TransportShutdownMode, TransportShutdownState,
};

pub use zero_rtt::{
    ZeroRttAdmissionDecision, ZeroRttAdmissionPolicy, ZeroRttAdmissionRejectionReason,
    ZeroRttReplayClass,
};
