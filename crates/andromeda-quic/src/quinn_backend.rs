//! H2-QUIC: Quinn backend for real QUIC transport
//!
//! This module implements the concrete QUIC transport using the quinn library,
//! adapting `quinn::Connection` and `quinn::Endpoint` to the abstract transport
//! trait boundary defined in `transport.rs`.
//!
//! ## Architecture
//!
//! ```text
//! quinn::Connection         Andromeda QUIC Layer
//! ─────────────────────────────────────────────────
//! open_uni_stream()  ────▶  open_uni_stream()
//! open_bidi_stream() ────▶  open_bidi_stream()
//! accept_uni()       ────▶  accept_uni()
//! accept_bidi()      ────▶  accept_bidi()
//! read_exact()       ────▶  frame codec
//! write_all()        ────▶  frame codec
//! ```
//!
//! ## Invariants
//!
//! - TLS handshake completes before any stream operations
//! - Certificate identity extracted before first RPC dispatch
//! - All stream IDs follow QUIC semantics (client-initiated even/odd)
//! - Connection remains open until explicit close

use std::net::SocketAddr;
use std::sync::Arc;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observe::CertificateIdentity;
use tokio::sync::Mutex;

use crate::mtls_identity::ParsedCertificate;
use crate::{FrameBytes, StreamRole, TransportEndpointMetadata, TransportMessage};

/// Adapter wrapping `quinn::Connection` for Andromeda frame transport.
///
/// This type bridges the quinn API to the abstract transport boundary.
pub struct QuinConnectionAdapter {
    inner: quinn::Connection,
    identity: Option<CertificateIdentity>,
}

impl QuinConnectionAdapter {
    /// Creates a new adapter for a quinn connection.
    pub fn new(conn: quinn::Connection, identity: Option<CertificateIdentity>) -> Self {
        Self {
            inner: conn,
            identity,
        }
    }

    /// Returns the peer address this connection is connected to/from.
    pub fn remote_addr(&self) -> SocketAddr {
        self.inner.remote_address()
    }

    /// Returns the certificate identity of the peer, if present.
    pub fn certificate_identity(&self) -> Option<&CertificateIdentity> {
        self.identity.as_ref()
    }

    /// Opens a unidirectional stream for sending.
    ///
    /// # Errors
    /// - `ConnectionError` if the connection is closed or stream limit exceeded
    pub async fn open_uni_stream(&mut self) -> AndromedaResult<UniStream> {
        self.inner
            .open_uni()
            .await
            .map(|s| UniStream { inner: s })
            .map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::ConnectionError,
                    format!("failed to open unidirectional stream: {}", e),
                )
            })
    }

    /// Opens a bidirectional stream.
    ///
    /// # Errors
    /// - `ConnectionError` if the connection is closed or stream limit exceeded
    pub async fn open_bidi_stream(&mut self) -> AndromedaResult<BidiStream> {
        self.inner
            .open_bi()
            .await
            .map(|(s, r)| BidiStream {
                send: s,
                recv: r,
            })
            .map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::ConnectionError,
                    format!("failed to open bidirectional stream: {}", e),
                )
            })
    }

    /// Accepts the next incoming unidirectional stream.
    ///
    /// # Errors
    /// - `ConnectionError` if the connection is closed
    pub async fn accept_uni_stream(&mut self) -> AndromedaResult<UniStream> {
        match self.inner.accept_uni().await {
            Ok(s) => Ok(UniStream { inner: s }),
            Err(e) => Err(AndromedaError::new(
                AndromedaErrorKind::ConnectionError,
                format!("failed to accept unidirectional stream: {}", e),
            )),
        }
    }

    /// Accepts the next incoming bidirectional stream.
    ///
    /// # Errors
    /// - `ConnectionError` if the connection is closed
    pub async fn accept_bidi_stream(&mut self) -> AndromedaResult<BidiStream> {
        match self.inner.accept_bi().await {
            Ok((s, r)) => Ok(BidiStream {
                send: s,
                recv: r,
            }),
            Err(e) => Err(AndromedaError::new(
                AndromedaErrorKind::ConnectionError,
                format!("failed to accept bidirectional stream: {}", e),
            )),
        }
    }

    /// Closes the connection with the given error code.
    pub fn close(&mut self, error_code: u32, reason: &[u8]) {
        self.inner.close(
            error_code.into(),
            reason,
        );
    }

    /// Waits for the connection to be fully closed.
    pub async fn closed(&mut self) {
        self.inner.closed().await;
    }

    /// Returns true if the connection is still open.
    pub fn is_open(&self) -> bool {
        !self.inner.is_closed()
    }
}

/// Adapter for a quinn unidirectional send stream.
pub struct UniStream {
    inner: quinn::SendStream,
}

impl UniStream {
    /// Writes data to the stream.
    ///
    /// # Errors
    /// - `WriteError` if the stream is closed or reset
    pub async fn write_all(&mut self, buf: &[u8]) -> AndromedaResult<()> {
        self.inner.write_all(buf).await.map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::IoError,
                format!("failed to write to unidirectional stream: {}", e),
            )
        })
    }

    /// Finishes writing to the stream.
    pub async fn finish(&mut self) -> AndromedaResult<()> {
        self.inner.finish().await.map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::IoError,
                format!("failed to finish unidirectional stream: {}", e),
            )
        })
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
        self.recv.read(buf).await.map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::IoError,
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
                AndromedaErrorKind::IoError,
                format!("failed to write to bidirectional stream: {}", e),
            )
        })
    }

    /// Finishes writing to the stream.
    pub async fn finish(&mut self) -> AndromedaResult<()> {
        self.send.finish().await.map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::IoError,
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
}

impl QuicServer {
    /// Creates a new QUIC server listening on the given address.
    ///
    /// # Arguments
    /// - `addr`: Listen address (e.g., "127.0.0.1:0" for ephemeral port)
    /// - `server_config`: Quinn server configuration with TLS certificates
    ///
    /// # Errors
    /// - `ConnectionError` if binding to the address fails
    pub fn new(addr: SocketAddr, server_config: quinn::ServerConfig) -> AndromedaResult<Self> {
        let socket = std::net::UdpSocket::bind(addr).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::ConnectionError,
                format!("failed to bind to {}: {}", addr, e),
            )
        })?;

        let local_addr = socket.local_addr().map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::ConnectionError,
                format!("failed to get local address: {}", e),
            )
        })?;

        let endpoint = quinn::Endpoint::new_with_abstract_socket(
            Default::default(),
            Some(server_config),
            socket.into(),
            Arc::new(quinn::TokioRuntime),
        )
        .map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::ConnectionError,
                format!("failed to create Quinn endpoint: {}", e),
            )
        })?;

        Ok(Self { endpoint })
    }

    /// Returns the local socket address the server is listening on.
    pub fn local_addr(&self) -> SocketAddr {
        self.endpoint.local_addr().unwrap_or_else(|_| {
            SocketAddr::from(([127, 0, 0, 1], 0))
        })
    }

    /// Accepts the next incoming connection.
    ///
    /// # Errors
    /// - `ConnectionError` if endpoint is closed
    pub async fn accept_connection(
        &self,
    ) -> AndromedaResult<QuinConnectionAdapter> {
        if let Some(connecting) = self.endpoint.accept().await {
            let conn = connecting.await.map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::ConnectionError,
                    format!("connection handshake failed: {}", e),
                )
            })?;

            // Extract certificate identity from peer certificates
            let identity = conn
                .peer_identity()
                .and_then(|cert_chain| {
                    cert_chain.iter().next().map(|cert_der| {
                        ParsedCertificate::parse(cert_der)
                            .ok()
                            .and_then(|pc| pc.certificate_identity())
                    })
                })
                .flatten();

            Ok(QuinConnectionAdapter::new(conn, identity))
        } else {
            Err(AndromedaError::new(
                AndromedaErrorKind::ConnectionError,
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
}

impl QuicClient {
    /// Creates a new QUIC client.
    ///
    /// # Arguments
    /// - `client_config`: Quinn client configuration with TLS settings
    ///
    /// # Errors
    /// - `ConnectionError` if endpoint creation fails
    pub fn new(client_config: quinn::ClientConfig) -> AndromedaResult<Self> {
        let socket = std::net::UdpSocket::bind("[::]:0").map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::ConnectionError,
                format!("failed to bind UDP socket: {}", e),
            )
        })?;

        let endpoint = quinn::Endpoint::new_with_abstract_socket(
            Default::default(),
            None,
            socket.into(),
            Arc::new(quinn::TokioRuntime),
        )
        .map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::ConnectionError,
                format!("failed to create Quinn endpoint: {}", e),
            )
        })?;

        Ok(Self {
            endpoint,
            client_config,
        })
    }

    /// Connects to a remote QUIC server.
    ///
    /// # Arguments
    /// - `addr`: Remote server address
    /// - `server_name`: TLS server name for certificate verification
    ///
    /// # Errors
    /// - `ConnectionError` if connection fails or TLS handshake fails
    pub async fn connect(
        &self,
        addr: SocketAddr,
        server_name: &str,
    ) -> AndromedaResult<QuinConnectionAdapter> {
        let connecting = self
            .endpoint
            .connect_with(self.client_config.clone(), addr, server_name)
            .map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::ConnectionError,
                    format!("failed to initiate connection to {}: {}", addr, e),
                )
            })?;

        let conn = connecting.await.map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::ConnectionError,
                format!("connection handshake failed: {}", e),
            )
        })?;

        // Extract server certificate identity
        let identity = conn
            .peer_identity()
            .and_then(|cert_chain| {
                cert_chain.iter().next().map(|cert_der| {
                    ParsedCertificate::parse(cert_der)
                        .ok()
                        .and_then(|pc| pc.certificate_identity())
                })
            })
            .flatten();

        Ok(QuinConnectionAdapter::new(conn, identity))
    }

    /// Closes the client endpoint.
    pub fn close(&self, error_code: u32, reason: &[u8]) {
        self.endpoint.close(error_code.into(), reason);
    }
}
