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

use std::net::SocketAddr;
use std::sync::Arc;

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CertificateIdentityStatus,
};
use andromeda_observe::{CertificateIdentity, SurfaceScope};

use crate::{
    CertificateContinuityDecision, CertificateContinuityPolicy, CertificateRotationDeclaration,
    ConnectionPool, ConnectionPoolKey, ConnectionPoolPolicy, PoolAdmission, PoolConnectionId,
    ReconnectPolicy, ReconnectState, RetryAdmissionDecision, RetryAdmissionPolicy,
    RetryIdempotency, SurfacePlane, ZeroRttAdmissionDecision, ZeroRttAdmissionPolicy,
    ZeroRttReplayClass, mtls_identity::RawCertificate,
};

/// Runtime-free policy bundle used by Quinn admission wiring.
///
/// The concrete Quinn layer can build this once per client runtime and feed it
/// handshake evidence, pool health, failure classification, and request
/// replay class without exposing Quinn handles to the policy model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuinnRuntimeAdmissionPolicy {
    pub pool: ConnectionPoolPolicy,
    pub reconnect: ReconnectPolicy,
    pub retry: RetryAdmissionPolicy,
    pub continuity: CertificateContinuityPolicy,
    pub zero_rtt: ZeroRttAdmissionPolicy,
}

impl QuinnRuntimeAdmissionPolicy {
    pub const fn conservative() -> Self {
        Self {
            pool: ConnectionPoolPolicy::conservative(),
            reconnect: ReconnectPolicy::conservative(),
            retry: RetryAdmissionPolicy::idempotent_only(),
            continuity: CertificateContinuityPolicy::strict(),
            zero_rtt: ZeroRttAdmissionPolicy::doctrine_v1_disabled(),
        }
    }
}

impl Default for QuinnRuntimeAdmissionPolicy {
    fn default() -> Self {
        Self::conservative()
    }
}

/// Deterministic Quinn admission planner.
///
/// The planner owns only pool metadata and policy decisions. It does not open
/// sockets, spawn timers, hold `quinn::Connection`, or perform TLS I/O.
#[derive(Debug, Clone)]
pub struct QuinnAdmissionPlanner {
    policy: QuinnRuntimeAdmissionPolicy,
    pool: ConnectionPool,
}

impl QuinnAdmissionPlanner {
    pub fn new(policy: QuinnRuntimeAdmissionPolicy) -> AndromedaResult<Self> {
        Ok(Self {
            pool: ConnectionPool::new(policy.pool)?,
            policy,
        })
    }

    pub fn conservative() -> AndromedaResult<Self> {
        Self::new(QuinnRuntimeAdmissionPolicy::conservative())
    }

    pub const fn policy(&self) -> QuinnRuntimeAdmissionPolicy {
        self.policy
    }

    pub const fn pool(&self) -> &ConnectionPool {
        &self.pool
    }

    pub fn admit_request(
        &mut self,
        request: QuinnAdmissionRequest<'_>,
    ) -> AndromedaResult<QuinnAdmissionDecision> {
        let certificate_status =
            QuinnCertificateStatusEvidence::evaluate(request.presented_identity, request.status)?;
        if certificate_status.is_denied() {
            return Err(certificate_status.into_security_error());
        }

        let pool_key =
            ConnectionPoolKey::from_server_identity(request.presented_identity, request.plane)?;

        let continuity = if let Some(previous_key) = request.previous_pool_key {
            Some(self.policy.continuity.validate_reconnect(
                previous_key,
                request.presented_identity,
                request.plane,
                request.declared_rotation,
            )?)
        } else {
            None
        };

        let zero_rtt = self.policy.zero_rtt.evaluate(request.zero_rtt_class);
        let retry = self.evaluate_retry(request.retry_after_failure)?;
        let pool_admission = if retry.allows_pool_admission() {
            Some(self.pool.admit_or_reuse(pool_key.clone(), request.now_ms)?)
        } else {
            None
        };

        Ok(QuinnAdmissionDecision {
            pool_key,
            pool_admission,
            certificate_status,
            continuity,
            zero_rtt,
            retry,
        })
    }

    fn evaluate_retry(
        &mut self,
        retry: Option<QuinnRetryRequest>,
    ) -> AndromedaResult<QuinnRetryOutcome> {
        let Some(retry) = retry else {
            return Ok(QuinnRetryOutcome::NotEvaluated);
        };

        if let Some(connection_id) = retry.previous_connection_id {
            self.pool.mark_unhealthy(connection_id)?;
        }

        if !retry.failure.is_retryable_by_transport() {
            return Ok(QuinnRetryOutcome::TransportNotRetryable {
                failure: retry.failure,
            });
        }

        self.policy
            .retry
            .admit_after_failure(
                retry.reconnect_state,
                self.policy.reconnect,
                retry.idempotency,
                retry.failed_attempt,
            )
            .map(QuinnRetryOutcome::RetryPolicy)
    }
}

/// Evidence available to the Quinn runtime before request dispatch.
#[derive(Debug, Clone, Copy)]
pub struct QuinnAdmissionRequest<'a> {
    pub presented_identity: &'a CertificateIdentity,
    pub status: CertificateIdentityStatus,
    pub plane: SurfacePlane,
    pub now_ms: u64,
    pub previous_pool_key: Option<&'a ConnectionPoolKey>,
    pub declared_rotation: Option<&'a CertificateRotationDeclaration>,
    pub zero_rtt_class: ZeroRttReplayClass,
    pub retry_after_failure: Option<QuinnRetryRequest>,
}

impl<'a> QuinnAdmissionRequest<'a> {
    pub const fn initial(
        presented_identity: &'a CertificateIdentity,
        plane: SurfacePlane,
        now_ms: u64,
        zero_rtt_class: ZeroRttReplayClass,
    ) -> Self {
        Self {
            presented_identity,
            status: CertificateIdentityStatus::Active,
            plane,
            now_ms,
            previous_pool_key: None,
            declared_rotation: None,
            zero_rtt_class,
            retry_after_failure: None,
        }
    }

    pub const fn with_certificate_status(mut self, status: CertificateIdentityStatus) -> Self {
        self.status = status;
        self
    }
}

/// Failure evidence projected from Quinn connection or stream errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuinnRetryRequest {
    pub previous_connection_id: Option<PoolConnectionId>,
    pub failure: QuinnNetworkFailureKind,
    pub reconnect_state: ReconnectState,
    pub idempotency: RetryIdempotency,
    pub failed_attempt: u32,
}

/// Runtime-independent QUIC failure classes used for retry admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuinnNetworkFailureKind {
    ConnectionLost,
    ConnectionReset,
    StreamReset,
    TimedOut,
    EndpointClosed,
    ApplicationClosed,
    AuthenticationFailed,
    ProtocolViolation,
}

impl QuinnNetworkFailureKind {
    pub const fn is_retryable_by_transport(self) -> bool {
        matches!(
            self,
            Self::ConnectionLost | Self::ConnectionReset | Self::StreamReset | Self::TimedOut
        )
    }
}

/// Unified decision the Quinn runtime can consume before opening/reusing a
/// connection and before admitting 0-RTT or retrying a failed dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuinnAdmissionDecision {
    pub pool_key: ConnectionPoolKey,
    pub pool_admission: Option<PoolAdmission>,
    pub certificate_status: QuinnCertificateStatusEvidence,
    pub continuity: Option<CertificateContinuityDecision>,
    pub zero_rtt: ZeroRttAdmissionDecision,
    pub retry: QuinnRetryOutcome,
}

/// Audit-ready certificate status evidence evaluated before Quinn pool
/// admission, reconnect, retry, or 0-RTT policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuinnCertificateStatusEvidence {
    pub fingerprint: String,
    pub subject: String,
    pub surface: SurfaceScope,
    pub status: CertificateIdentityStatus,
    pub outcome: QuinnCertificateStatusOutcome,
    pub reason: QuinnCertificateStatusReason,
}

impl QuinnCertificateStatusEvidence {
    pub fn evaluate(
        identity: &CertificateIdentity,
        status: CertificateIdentityStatus,
    ) -> AndromedaResult<Self> {
        if !identity.has_identity_evidence() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Security,
                "QUIC mTLS certificate status admission requires fingerprint and subject evidence",
            ));
        }

        let (outcome, reason) = match status {
            CertificateIdentityStatus::Active => (
                QuinnCertificateStatusOutcome::Allowed,
                QuinnCertificateStatusReason::Active,
            ),
            CertificateIdentityStatus::Disabled => (
                QuinnCertificateStatusOutcome::Denied,
                QuinnCertificateStatusReason::CertificateDisabled,
            ),
            CertificateIdentityStatus::Revoked => (
                QuinnCertificateStatusOutcome::Denied,
                QuinnCertificateStatusReason::CertificateRevoked,
            ),
        };

        Ok(Self {
            fingerprint: identity.fingerprint.clone(),
            subject: identity.subject.clone(),
            surface: identity.surface,
            status,
            outcome,
            reason,
        })
    }

    pub const fn is_allowed(&self) -> bool {
        matches!(self.outcome, QuinnCertificateStatusOutcome::Allowed)
    }

    pub const fn is_denied(&self) -> bool {
        matches!(self.outcome, QuinnCertificateStatusOutcome::Denied)
    }

    pub fn into_security_error(self) -> AndromedaError {
        AndromedaError::new(
            AndromedaErrorKind::Security,
            format!(
                "QUIC mTLS certificate admission denied: reason={} status={} surface={} fingerprint={} subject_present={}",
                self.reason.as_str(),
                self.status.as_str(),
                surface_scope_label(self.surface),
                self.fingerprint,
                !self.subject.trim().is_empty(),
            ),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuinnCertificateStatusOutcome {
    Allowed,
    Denied,
}

impl QuinnCertificateStatusOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::Denied => "denied",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuinnCertificateStatusReason {
    Active,
    CertificateDisabled,
    CertificateRevoked,
}

impl QuinnCertificateStatusReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::CertificateDisabled => "certificate_disabled",
            Self::CertificateRevoked => "certificate_revoked",
        }
    }
}

/// Retry decision after Quinn failures have been classified at the transport
/// boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuinnRetryOutcome {
    NotEvaluated,
    TransportNotRetryable { failure: QuinnNetworkFailureKind },
    RetryPolicy(RetryAdmissionDecision),
}

impl QuinnRetryOutcome {
    pub const fn allows_pool_admission(self) -> bool {
        matches!(
            self,
            Self::NotEvaluated
                | Self::RetryPolicy(RetryAdmissionDecision::Admit {
                    next_attempt: _,
                    delay_ms: _,
                })
        )
    }
}

/// Adapter wrapping `quinn::Connection` for Andromeda frame transport.
///
/// This type bridges the quinn API to the abstract transport boundary.
pub struct QuinnConnectionAdapter {
    inner: quinn::Connection,
    identity: CertificateIdentity,
    peer_certificates: Vec<RawCertificate>,
}

impl QuinnConnectionAdapter {
    /// Creates a new adapter for a quinn connection.
    pub fn new(conn: quinn::Connection, identity: CertificateIdentity) -> Self {
        Self::with_peer_certificates(conn, identity, Vec::new())
    }

    fn with_peer_certificates(
        conn: quinn::Connection,
        identity: CertificateIdentity,
        peer_certificates: Vec<RawCertificate>,
    ) -> Self {
        Self {
            inner: conn,
            identity,
            peer_certificates,
        }
    }

    /// Returns the peer address this connection is connected to/from.
    pub fn remote_addr(&self) -> SocketAddr {
        self.inner.remote_address()
    }

    /// Returns the authenticated certificate identity of the peer.
    pub const fn certificate_identity(&self) -> &CertificateIdentity {
        &self.identity
    }

    /// Builds the runtime-free connection pool key from the authenticated
    /// server identity extracted from the Quinn handshake.
    pub fn connection_pool_key(&self, plane: SurfacePlane) -> AndromedaResult<ConnectionPoolKey> {
        ConnectionPoolKey::from_server_identity(&self.identity, plane)
    }

    /// Returns the raw peer certificate chain exposed by Quinn.
    pub fn peer_certificate_chain(&self) -> &[RawCertificate] {
        &self.peer_certificates
    }

    /// Opens a unidirectional stream for sending.
    ///
    /// # Errors
    /// - `Transport` if the connection is closed or stream limit exceeded
    pub async fn open_uni_stream(&mut self) -> AndromedaResult<UniStream> {
        self.inner
            .open_uni()
            .await
            .map(|s| UniStream {
                inner: UniStreamInner::Send(s),
            })
            .map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Transport,
                    format!("failed to open unidirectional stream: {}", e),
                )
            })
    }

    /// Opens a bidirectional stream.
    ///
    /// # Errors
    /// - `Transport` if the connection is closed or stream limit exceeded
    pub async fn open_bidi_stream(&mut self) -> AndromedaResult<BidiStream> {
        self.inner
            .open_bi()
            .await
            .map(|(s, r)| BidiStream { send: s, recv: r })
            .map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Transport,
                    format!("failed to open bidirectional stream: {}", e),
                )
            })
    }

    /// Accepts the next incoming unidirectional stream.
    ///
    /// # Errors
    /// - `Transport` if the connection is closed
    pub async fn accept_uni_stream(&mut self) -> AndromedaResult<UniStream> {
        match self.inner.accept_uni().await {
            Ok(s) => Ok(UniStream {
                inner: UniStreamInner::Recv(s),
            }),
            Err(e) => Err(AndromedaError::new(
                AndromedaErrorKind::Transport,
                format!("failed to accept unidirectional stream: {}", e),
            )),
        }
    }

    /// Accepts the next incoming bidirectional stream.
    ///
    /// # Errors
    /// - `Transport` if the connection is closed
    pub async fn accept_bidi_stream(&mut self) -> AndromedaResult<BidiStream> {
        match self.inner.accept_bi().await {
            Ok((s, r)) => Ok(BidiStream { send: s, recv: r }),
            Err(e) => Err(AndromedaError::new(
                AndromedaErrorKind::Transport,
                format!("failed to accept bidirectional stream: {}", e),
            )),
        }
    }

    /// Closes the connection with the given error code.
    pub fn close(&mut self, error_code: u32, reason: &[u8]) {
        self.inner.close(error_code.into(), reason);
    }

    /// Waits for the connection to be fully closed.
    pub async fn closed(&mut self) {
        self.inner.closed().await;
    }

    /// Returns true if the connection is still open.
    pub fn is_open(&self) -> bool {
        self.inner.close_reason().is_none()
    }
}

/// Adapter for a quinn unidirectional send stream.
pub struct UniStream {
    inner: UniStreamInner,
}

enum UniStreamInner {
    Send(quinn::SendStream),
    #[allow(dead_code)]
    Recv(quinn::RecvStream),
}

impl UniStream {
    /// Writes data to the stream.
    ///
    /// # Errors
    /// - `WriteError` if the stream is closed or reset
    pub async fn write_all(&mut self, buf: &[u8]) -> AndromedaResult<()> {
        match &mut self.inner {
            UniStreamInner::Send(stream) => stream.write_all(buf).await.map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Transport,
                    format!("failed to write to unidirectional stream: {}", e),
                )
            }),
            UniStreamInner::Recv(_) => Err(AndromedaError::new(
                AndromedaErrorKind::Transport,
                "cannot write to incoming unidirectional receive stream",
            )),
        }
    }

    /// Finishes writing to the stream.
    pub async fn finish(&mut self) -> AndromedaResult<()> {
        match &mut self.inner {
            UniStreamInner::Send(stream) => stream.finish().map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Transport,
                    format!("failed to finish unidirectional stream: {}", e),
                )
            }),
            UniStreamInner::Recv(_) => Err(AndromedaError::new(
                AndromedaErrorKind::Transport,
                "cannot finish incoming unidirectional receive stream",
            )),
        }
    }
}

/// Adapter for a quinn bidirectional stream.
pub struct BidiStream {
    send: quinn::SendStream,
    recv: quinn::RecvStream,
}

impl BidiStream {
    /// Reads from the bidirectional stream into a buffer.
    ///
    /// # Errors
    /// - `ReadError` if the stream is closed or reset
    pub async fn read(&mut self, buf: &mut [u8]) -> AndromedaResult<usize> {
        self.recv
            .read(buf)
            .await
            .map(|n| n.unwrap_or(0))
            .map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Transport,
                    format!("failed to read from bidirectional stream: {}", e),
                )
            })
    }

    /// Writes data to the stream.
    ///
    /// # Errors
    /// - `WriteError` if the stream is closed or reset
    pub async fn write_all(&mut self, buf: &[u8]) -> AndromedaResult<()> {
        self.send.write_all(buf).await.map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Transport,
                format!("failed to write to bidirectional stream: {}", e),
            )
        })
    }

    /// Finishes writing to the stream.
    pub async fn finish(&mut self) -> AndromedaResult<()> {
        self.send.finish().map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Transport,
                format!("failed to finish bidirectional stream: {}", e),
            )
        })
    }
}

/// Quinn-based QUIC server.
///
/// Listens for incoming connections on a socket and accepts them,
/// extracting certificate identity for authorization.
pub struct QuicServer {
    endpoint: quinn::Endpoint,
    required_scope: SurfaceScope,
}

impl QuicServer {
    /// Creates a new QUIC server listening on the given address.
    ///
    /// # Arguments
    /// - `addr`: Listen address (e.g., "127.0.0.1:0" for ephemeral port)
    /// - `server_config`: Quinn server configuration with TLS certificates
    ///
    /// # Errors
    /// - `Transport` if binding to the address fails
    pub fn new(addr: SocketAddr, server_config: quinn::ServerConfig) -> AndromedaResult<Self> {
        Self::with_required_scope(addr, server_config, SurfaceScope::Application)
    }

    /// Creates a new QUIC server with the certificate surface scope to bind
    /// to accepted peer identities.
    pub fn with_required_scope(
        addr: SocketAddr,
        server_config: quinn::ServerConfig,
        required_scope: SurfaceScope,
    ) -> AndromedaResult<Self> {
        let socket = std::net::UdpSocket::bind(addr).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Transport,
                format!("failed to bind to {}: {}", addr, e),
            )
        })?;

        let endpoint = quinn::Endpoint::new(
            Default::default(),
            Some(server_config),
            socket,
            Arc::new(quinn::TokioRuntime),
        )
        .map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Transport,
                format!("failed to create Quinn endpoint: {}", e),
            )
        })?;

        Ok(Self {
            endpoint,
            required_scope,
        })
    }

    /// Returns the local socket address the server is listening on.
    pub fn local_addr(&self) -> SocketAddr {
        self.endpoint
            .local_addr()
            .unwrap_or_else(|_| SocketAddr::from(([127, 0, 0, 1], 0)))
    }

    /// Accepts the next incoming connection.
    ///
    /// # Errors
    /// - `Transport` if endpoint is closed
    pub async fn accept_connection(&self) -> AndromedaResult<QuinnConnectionAdapter> {
        if let Some(connecting) = self.endpoint.accept().await {
            let conn = connecting.await.map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Transport,
                    format!("connection handshake failed: {}", e),
                )
            })?;

            let peer_certificates = extract_peer_certificates(&conn);
            let identity = require_certificate_identity(&peer_certificates, self.required_scope)?;

            Ok(QuinnConnectionAdapter::with_peer_certificates(
                conn,
                identity,
                peer_certificates,
            ))
        } else {
            Err(AndromedaError::new(
                AndromedaErrorKind::Transport,
                "endpoint closed".to_string(),
            ))
        }
    }

    /// Closes the server.
    pub fn close(&self, error_code: u32, reason: &[u8]) {
        self.endpoint.close(error_code.into(), reason);
    }
}

/// Quinn-based QUIC client.
///
/// Connects to remote QUIC servers and manages the connection lifecycle.
pub struct QuicClient {
    endpoint: quinn::Endpoint,
    client_config: quinn::ClientConfig,
    required_scope: SurfaceScope,
}

impl QuicClient {
    /// Creates a new QUIC client.
    ///
    /// # Arguments
    /// - `client_config`: Quinn client configuration with TLS settings
    ///
    /// # Errors
    /// - `Transport` if endpoint creation fails
    pub fn new(client_config: quinn::ClientConfig) -> AndromedaResult<Self> {
        Self::with_required_scope(client_config, SurfaceScope::Application)
    }

    /// Creates a new QUIC client with the certificate surface scope to bind
    /// to the connected server identity.
    pub fn with_required_scope(
        client_config: quinn::ClientConfig,
        required_scope: SurfaceScope,
    ) -> AndromedaResult<Self> {
        let socket =
            std::net::UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))).map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Transport,
                    format!("failed to bind UDP socket: {}", e),
                )
            })?;

        let endpoint = quinn::Endpoint::new(
            Default::default(),
            None,
            socket,
            Arc::new(quinn::TokioRuntime),
        )
        .map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Transport,
                format!("failed to create Quinn endpoint: {}", e),
            )
        })?;

        Ok(Self {
            endpoint,
            client_config,
            required_scope,
        })
    }

    /// Connects to a remote QUIC server.
    ///
    /// # Arguments
    /// - `addr`: Remote server address
    /// - `server_name`: TLS server name for certificate verification
    ///
    /// # Errors
    /// - `Transport` if connection fails or TLS handshake fails
    pub async fn connect(
        &self,
        addr: SocketAddr,
        server_name: &str,
    ) -> AndromedaResult<QuinnConnectionAdapter> {
        let connecting = self
            .endpoint
            .connect_with(self.client_config.clone(), addr, server_name)
            .map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Transport,
                    format!("failed to initiate connection to {}: {}", addr, e),
                )
            })?;

        let conn = connecting.await.map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Transport,
                format!("connection handshake failed: {}", e),
            )
        })?;

        let peer_certificates = extract_peer_certificates(&conn);
        let identity = require_certificate_identity(&peer_certificates, self.required_scope)?;

        Ok(QuinnConnectionAdapter::with_peer_certificates(
            conn,
            identity,
            peer_certificates,
        ))
    }

    /// Closes the client endpoint.
    pub fn close(&self, error_code: u32, reason: &[u8]) {
        self.endpoint.close(error_code.into(), reason);
    }
}

fn extract_peer_certificates(conn: &quinn::Connection) -> Vec<RawCertificate> {
    let Some(peer_identity) = conn.peer_identity() else {
        return Vec::new();
    };

    let Ok(cert_chain) =
        peer_identity.downcast::<Vec<rustls::pki_types::CertificateDer<'static>>>()
    else {
        return Vec::new();
    };

    cert_chain
        .iter()
        .map(|cert| RawCertificate::new(cert.as_ref().to_vec()))
        .collect()
}

const fn surface_scope_label(surface: SurfaceScope) -> &'static str {
    match surface {
        SurfaceScope::Application => "application",
        SurfaceScope::Administration => "administration",
        SurfaceScope::Cluster => "cluster",
        SurfaceScope::BackupAgent => "backup_agent",
        SurfaceScope::MonitoringAgent => "monitoring_agent",
    }
}

fn require_certificate_identity(
    peer_certificates: &[RawCertificate],
    required_scope: SurfaceScope,
) -> AndromedaResult<CertificateIdentity> {
    let Some(cert) = peer_certificates.first() else {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Security,
            "QUIC mTLS handshake did not expose an authenticated peer certificate",
        ));
    };

    cert.to_certificate_identity(required_scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_certificate_identity_rejects_missing_peer_certificate() {
        let err = require_certificate_identity(&[], SurfaceScope::Application).unwrap_err();

        assert_eq!(err.kind(), AndromedaErrorKind::Security);
        assert!(
            err.message().contains("mTLS"),
            "missing peer certificate must fail closed as mTLS security evidence"
        );
    }
}
