//! Quinn backend for real QUIC transport.
//!
//! This module implements the concrete QUIC transport using the quinn library,
//! adapting `quinn::Connection` and `quinn::Endpoint` to the abstract transport
//! trait boundary defined in `transport.rs`.
//!
//! ## Invariants
//!
//! - TLS handshake completes before any stream operations
//! - Certificate identity extracted before first RPC dispatch
//! - All stream IDs follow QUIC semantics (client-initiated even/odd)
//! - Connection remains open until explicit close

mod admission;
mod client;
mod connection_adapter;
mod endpoint;
mod peer_certificate;
mod server;
mod surface;

pub use admission::{
    QuinnAdmissionDecision, QuinnAdmissionPlanner, QuinnAdmissionRequest,
    QuinnCertificateStatusEvidence, QuinnCertificateStatusOutcome, QuinnCertificateStatusReason,
    QuinnNetworkFailureKind, QuinnRetryOutcome, QuinnRetryRequest, QuinnRuntimeAdmissionPolicy,
};
pub use client::QuicClient;
pub use connection_adapter::{BidiStream, QuinnConnectionAdapter, UniStream};
pub use server::QuicServer;
pub use surface::QuinnRuntimeSurface;
