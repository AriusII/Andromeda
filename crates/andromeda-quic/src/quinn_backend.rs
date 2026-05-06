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

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observe::CertificateIdentity;

use crate::mtls_identity::RawCertificate;

/// Adapter wrapping `quinn::Connection` for Andromeda frame transport.
///
/// This type bridges the quinn API to the abstract transport boundary.
pub struct QuinnConnectionAdapter {
    inner: quinn::Connection,
    identity: Option<CertificateIdentity>,
    peer_certificates: Vec<RawCertificate>,
}

impl QuinnConnectionAdapter {
    /// Creates a new adapter for a quinn connection.
    pub fn new(conn: quinn::Connection, identity: Option<CertificateIdentity>) -> Self {
        Self::with_peer_certificates(conn, identity, Vec::new())
    }

    fn with_peer_certificates(
        conn: quinn::Connection,
        identity: Option<CertificateIdentity>,
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

    /// Returns the certificate identity of the peer, if present.
    pub fn certificate_identity(&self) -> Option<&CertificateIdentity> {
        self.identity.as_ref()
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

        Ok(Self { endpoint })
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
            let identity = None;

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
        let identity = None;

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
