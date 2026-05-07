use std::net::SocketAddr;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observe::CertificateIdentity;

use crate::{ConnectionPoolKey, SurfacePlane, mtls_identity::RawCertificate};

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

    pub(super) fn with_peer_certificates(
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
