use std::net::SocketAddr;
use std::sync::Arc;

use andromeda_core::SurfaceScope;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use super::connection_adapter::QuinnConnectionAdapter;
use super::peer_certificate::{extract_peer_certificates, require_certificate_identity};

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
