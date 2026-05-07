use std::net::SocketAddr;
use std::sync::Arc;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observe::SurfaceScope;

use super::connection_adapter::QuinnConnectionAdapter;
use super::peer_certificate::{extract_peer_certificates, require_certificate_identity};

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
