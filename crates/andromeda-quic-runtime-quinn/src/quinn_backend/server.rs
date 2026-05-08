use std::net::SocketAddr;
use std::sync::Arc;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use super::connection_adapter::QuinnConnectionAdapter;
use super::peer_certificate::{extract_peer_certificates, require_certificate_identity};
use super::surface::QuinnRuntimeSurface;

/// Quinn-based QUIC server.
///
/// Listens for incoming connections on a socket and accepts them,
/// extracting certificate identity for authorization.
pub struct QuicServer {
    endpoint: quinn::Endpoint,
    runtime_surface: QuinnRuntimeSurface,
}

impl QuicServer {
    /// Creates a test QUIC server on the Application surface.
    ///
    /// Production wiring should use [`Self::for_surface`] so the runtime
    /// surface is explicit at construction time.
    #[cfg(any(test, feature = "insecure-test-tls"))]
    #[doc(hidden)]
    pub fn new(addr: SocketAddr, server_config: quinn::ServerConfig) -> AndromedaResult<Self> {
        Self::for_surface(
            addr,
            server_config,
            QuinnRuntimeSurface::for_plane(andromeda_quic::SurfacePlane::Application),
        )
    }

    /// Creates a QUIC server listening on the given address for an explicit
    /// runtime surface.
    ///
    /// # Arguments
    /// - `addr`: Listen address (e.g., "127.0.0.1:0" for ephemeral port)
    /// - `server_config`: Quinn server configuration with TLS certificates
    /// - `runtime_surface`: selected transport plane and required peer scope
    ///
    /// # Errors
    /// - `Transport` if binding to the address fails
    pub fn for_surface(
        addr: SocketAddr,
        server_config: quinn::ServerConfig,
        runtime_surface: QuinnRuntimeSurface,
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
            runtime_surface,
        })
    }

    /// Returns the selected runtime surface for this endpoint.
    pub const fn runtime_surface(&self) -> QuinnRuntimeSurface {
        self.runtime_surface
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
            let identity = require_certificate_identity(
                &peer_certificates,
                self.runtime_surface.required_scope(),
            )?;

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
