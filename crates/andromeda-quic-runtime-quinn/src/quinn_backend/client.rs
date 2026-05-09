use std::net::SocketAddr;

use andromeda_error::AndromedaResult;

use super::connection_adapter::QuinnConnectionAdapter;
use super::endpoint::{endpoint_from_socket, transport_error};
use super::peer_certificate::authenticated_connection_adapter;
use super::surface::QuinnRuntimeSurface;

/// Quinn-based QUIC client.
///
/// Connects to remote QUIC servers and manages the connection lifecycle.
pub struct QuicClient {
    endpoint: quinn::Endpoint,
    client_config: quinn::ClientConfig,
    runtime_surface: QuinnRuntimeSurface,
}

impl QuicClient {
    /// Creates a test QUIC client on the Application surface.
    ///
    /// Production wiring should use [`Self::for_surface`] so the runtime
    /// surface is explicit at construction time.
    #[cfg(test)]
    #[doc(hidden)]
    pub fn new(client_config: quinn::ClientConfig) -> AndromedaResult<Self> {
        Self::for_surface(
            client_config,
            QuinnRuntimeSurface::for_plane(andromeda_quic::SurfacePlane::Application),
        )
    }

    /// Creates a QUIC client for an explicit runtime surface.
    ///
    /// # Arguments
    /// - `client_config`: Quinn client configuration with TLS settings
    /// - `runtime_surface`: selected transport plane and required peer scope
    ///
    /// # Errors
    /// - `Transport` if endpoint creation fails
    pub fn for_surface(
        client_config: quinn::ClientConfig,
        runtime_surface: QuinnRuntimeSurface,
    ) -> AndromedaResult<Self> {
        let socket = std::net::UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0)))
            .map_err(|e| transport_error(format!("failed to bind UDP socket: {e}")))?;

        let endpoint = endpoint_from_socket(None, socket)?;

        Ok(Self {
            endpoint,
            client_config,
            runtime_surface,
        })
    }

    /// Returns the selected runtime surface for this endpoint.
    pub const fn runtime_surface(&self) -> QuinnRuntimeSurface {
        self.runtime_surface
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
                transport_error(format!("failed to initiate connection to {addr}: {e}"))
            })?;

        let conn = connecting
            .await
            .map_err(|e| transport_error(format!("connection handshake failed: {e}")))?;

        authenticated_connection_adapter(conn, self.runtime_surface.required_scope())
    }

    /// Closes the client endpoint.
    pub fn close(&self, error_code: u32, reason: &[u8]) {
        self.endpoint.close(error_code.into(), reason);
    }
}
