#![forbid(unsafe_code)]

//! Andromeda QUIC transport contracts.
//!
//! The default API is runtime-free: frame encoding, stream role validation,
//! lifecycle gating, backpressure, RPC dispatch, and HA/DR stream allocation are
//! modelled without exposing a concrete QUIC backend. Quinn-backed TLS and
//! network adapters live in the separate `andromeda-quic-runtime-quinn` crate.
//!
//! Critical invariants:
//! - lifecycle state gates handshake, active dispatch, drain, and close.
//! - QUIC surface-plane checks gate transport admission before RPC dispatch;
//! - concrete Quinn/TLS behavior remains in `andromeda-quic-runtime-quinn`.

pub mod catalog_manifest_resolution;
mod connection;
pub mod procedure_gateway;
mod reconnect;
mod zero_rtt;

mod stream_concurrency;
pub mod transport;

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

pub use procedure_gateway::ProcedureGateway;

pub use reconnect::{
    CertificateContinuityDecision, CertificateContinuityPolicy, CertificateRotationDeclaration,
    ConnectionPool, ConnectionPoolKey, ConnectionPoolPolicy, MAX_POOL_CONNECTIONS_PER_KEY,
    MAX_POOL_IDLE_TIMEOUT_MS, PoolAdmission, PoolAdmissionKind, PoolConnectionId, PooledConnection,
    PooledConnectionHealth, ReconnectAttemptTrace, ReconnectDecision, ReconnectPolicy,
    ReconnectState, RetryAdmissionDecision, RetryAdmissionPolicy, RetryIdempotency,
    RetryRejectionReason,
};

pub use catalog_manifest_resolution::{
    CatalogManifestResolutionGateway, CatalogManifestResolutionRuntime,
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
