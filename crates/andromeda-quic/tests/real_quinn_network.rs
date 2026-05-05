//! H2-QUIC: Real Quinn Network Integration Tests
//!
//! This test suite validates real QUIC transport with:
//! - Server listening and accepting connections
//! - Client connection establishment with TLS
//! - Frame send/receive over QUIC streams
//! - Certificate identity extraction
//! - Concurrent connections
//! - End-to-end frame exchange

#![cfg(feature = "runtime-quinn")]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use andromeda_quic::frame::{FrameType, FRAME_HEADER_CRC_UNCHECKED};
use andromeda_quic::{
    FrameBytes, FrameCodec, FrameHeader, SurfacePlane,
    quinn_backend::{QuicClient, QuicServer},
    quinn_tls::{ClientTlsConfig, ServerTlsConfig},
};
use tokio::time::timeout;

// ============================================================================
// Helper Functions
// ============================================================================

/// Creates an ephemeral server TLS configuration (self-signed cert for testing).
fn create_test_server_tls() -> andromeda_core::AndromedaResult<quinn::ServerConfig> {
    let tls_config = ServerTlsConfig::ephemeral(vec!["localhost".to_string()])?;
    Ok(tls_config.into_quinn_config())
}

/// Creates an insecure client TLS configuration for testing.
fn create_test_client_tls() -> quinn::ClientConfig {
    ClientTlsConfig::insecure()
}

/// Allocates an ephemeral local socket address.
fn allocate_test_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
}

/// Writes a frame to a stream.
async fn write_frame(
    stream: &mut (impl tokio::io::AsyncWriteExt + Unpin),
    frame: &FrameBytes,
) -> andromeda_core::AndromedaResult<()> {
    use tokio::io::AsyncWriteExt;

    let encoded = FrameCodec::encode(frame)?;
    stream.write_all(&encoded).await.map_err(|e| {
        andromeda_core::AndromedaError::new(
            andromeda_core::AndromedaErrorKind::IoError,
            format!("failed to write frame: {}", e),
        )
    })?;
    Ok(())
}

/// Reads a frame from a stream.
async fn read_frame(
    stream: &mut (impl tokio::io::AsyncReadExt + Unpin),
) -> andromeda_core::AndromedaResult<FrameBytes> {
    use tokio::io::AsyncReadExt;

    let mut header_buf = [0u8; 52]; // Frame header is 52 bytes (based on FRAME_CODEC_HEADER_LEN)
    stream.read_exact(&mut header_buf).await.map_err(|e| {
        andromeda_core::AndromedaError::new(
            andromeda_core::AndromedaErrorKind::IoError,
            format!("failed to read frame header: {}", e),
        )
    })?;

    // Parse header to get payload length
    let (frame, _consumed) = FrameCodec::scan_one(&header_buf)?;
    let payload_len = frame.header.payload_length as usize;

    // Read payload if needed
    if payload_len > 0 {
        let mut payload = vec![0u8; payload_len];
        stream.read_exact(&mut payload).await.map_err(|e| {
            andromeda_core::AndromedaError::new(
                andromeda_core::AndromedaErrorKind::IoError,
                format!("failed to read frame payload: {}", e),
            )
        })?;

        let mut full_frame = header_buf.to_vec();
        full_frame.extend_from_slice(&payload);
        FrameCodec::decode(&full_frame)
    } else {
        FrameCodec::decode(&header_buf)
    }
}

// ============================================================================
// Test: Server Startup and Address
// ============================================================================

#[tokio::test]
async fn test_server_startup_and_listen_address() -> andromeda_core::AndromedaResult<()> {
    let addr = allocate_test_address();
    let server_tls = create_test_server_tls()?;

    let server = QuicServer::new(addr, server_tls)?;
    let local_addr = server.local_addr();

    // Verify server has a valid address
    assert_eq!(local_addr.ip(), std::net::IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert_ne!(local_addr.port(), 0, "Port should be allocated");

    server.close(0, b"test complete");
    Ok(())
}

// ============================================================================
// Test: Client Connection Establishment with TLS Handshake
// ============================================================================

#[tokio::test]
async fn test_client_connection_tls_negotiation(
) -> andromeda_core::AndromedaResult<()> {
    let server_addr = allocate_test_address();
    let server_tls = create_test_server_tls()?;
    let server = QuicServer::new(server_addr, server_tls)?;
    let listen_addr = server.local_addr();

    // Spawn server to accept a single connection
    let server_handle = tokio::spawn(async move {
        timeout(Duration::from_secs(5), server.accept_connection()).await
    });

    // Client connects
    let client_tls = create_test_client_tls();
    let client = QuicClient::new(client_tls)?;

    let conn = timeout(
        Duration::from_secs(5),
        client.connect(listen_addr, "localhost"),
    )
    .await??;

    assert!(conn.is_open());
    assert_eq!(conn.remote_addr(), listen_addr);

    // Verify server accepted the connection
    let server_conn = timeout(Duration::from_secs(5), server_handle).await???;
    assert!(server_conn.is_open());

    Ok(())
}

// ============================================================================
// Test: Echo Frame Exchange (Unidirectional Stream)
// ============================================================================

#[tokio::test]
async fn test_frame_echo_unidirectional() -> andromeda_core::AndromedaResult<()> {
    let server_addr = allocate_test_address();
    let server_tls = create_test_server_tls()?;
    let server = QuicServer::new(server_addr, server_tls)?;
    let listen_addr = server.local_addr();

    // Server task: accept connection, read frame, ignore (echo happens via client send)
    let server_handle = tokio::spawn(async move {
        if let Ok(mut conn) = timeout(Duration::from_secs(5), server.accept_connection()).await {
            if let Ok(result) = conn.await {
                // Just accept the connection; client will send frame
                return Ok::<_, Box<dyn std::error::Error>>(());
            }
        }
        Err("server timeout".into())
    });

    // Client connects and sends a frame
    let client_tls = create_test_client_tls();
    let client = QuicClient::new(client_tls)?;
    let mut conn = timeout(
        Duration::from_secs(5),
        client.connect(listen_addr, "localhost"),
    )
    .await??;

    // Open unidirectional stream and send a frame
    let mut stream = conn.open_uni_stream().await?;

    let test_payload = b"Hello, QUIC!".to_vec();
    let frame = FrameBytes {
        header: FrameHeader {
            frame_type: FrameType::RpcMetadata,
            request_id: 42,
            session_id: 100,
            tx_id: None,
            payload_length: test_payload.len() as u64,
            flags: 0,
            header_crc: FRAME_HEADER_CRC_UNCHECKED,
        },
        payload: test_payload.clone(),
    };

    let encoded = FrameCodec::encode(&frame)?;
    stream.write_all(&encoded).await?;
    stream.finish().await?;

    // Wait for server to complete
    timeout(Duration::from_secs(5), server_handle).await???;

    Ok(())
}

// ============================================================================
// Test: Certificate Identity Extraction
// ============================================================================

#[tokio::test]
async fn test_certificate_identity_extraction() -> andromeda_core::AndromedaResult<()> {
    let server_addr = allocate_test_address();
    let server_tls = create_test_server_tls()?;
    let server = QuicServer::new(server_addr, server_tls)?;
    let listen_addr = server.local_addr();

    // Server task: accept and verify certificate identity
    let server_handle = tokio::spawn(async move {
        if let Ok(conn) = timeout(Duration::from_secs(5), server.accept_connection()).await {
            if let Ok(conn) = conn {
                // Certificate should be extracted (self-signed for testing)
                let identity = conn.certificate_identity();
                return Ok::<_, Box<dyn std::error::Error>>(identity.is_some());
            }
        }
        Err("server timeout".into())
    });

    // Client connects
    let client_tls = create_test_client_tls();
    let client = QuicClient::new(client_tls)?;
    let _conn = timeout(
        Duration::from_secs(5),
        client.connect(listen_addr, "localhost"),
    )
    .await??;

    let has_identity = timeout(Duration::from_secs(5), server_handle).await???;
    assert!(
        has_identity,
        "Server should extract certificate identity from connection"
    );

    Ok(())
}

// ============================================================================
// Test: Concurrent Connections
// ============================================================================

#[tokio::test]
async fn test_concurrent_connections() -> andromeda_core::AndromedaResult<()> {
    let server_addr = allocate_test_address();
    let server_tls = create_test_server_tls()?;
    let server = Arc::new(QuicServer::new(server_addr, server_tls)?);
    let listen_addr = server.local_addr();

    let accepted_count = Arc::new(AtomicU32::new(0));

    // Spawn server task to accept 10 connections
    let server_clone = server.clone();
    let count_clone = accepted_count.clone();
    let accept_handle = tokio::spawn(async move {
        for _ in 0..10 {
            if let Ok(conn) = timeout(Duration::from_secs(10), server_clone.accept_connection())
                .await
            {
                if let Ok(_) = conn {
                    count_clone.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
    });

    // Spawn 10 clients connecting concurrently
    let mut client_handles = vec![];
    for _ in 0..10 {
        let listen_addr_copy = listen_addr;
        let handle = tokio::spawn(async move {
            let client_tls = create_test_client_tls();
            if let Ok(client) = QuicClient::new(client_tls) {
                let _conn = timeout(
                    Duration::from_secs(5),
                    client.connect(listen_addr_copy, "localhost"),
                )
                .await;
            }
        });
        client_handles.push(handle);
    }

    // Wait for all clients
    for handle in client_handles {
        let _ = timeout(Duration::from_secs(10), handle).await;
    }

    // Wait for server to accept all
    let _ = timeout(Duration::from_secs(10), accept_handle).await;

    assert_eq!(
        accepted_count.load(Ordering::SeqCst),
        10,
        "Server should accept all 10 connections"
    );

    Ok(())
}

// ============================================================================
// Test: Bidirectional Stream Communication
// ============================================================================

#[tokio::test]
async fn test_bidirectional_stream_communication() -> andromeda_core::AndromedaResult<()> {
    let server_addr = allocate_test_address();
    let server_tls = create_test_server_tls()?;
    let server = QuicServer::new(server_addr, server_tls)?;
    let listen_addr = server.local_addr();

    // Server: accept connection and bidirectional stream
    let server_handle = tokio::spawn(async move {
        if let Ok(mut conn) = timeout(Duration::from_secs(5), server.accept_connection()).await {
            if let Ok(mut conn) = conn {
                if let Ok(mut stream) = timeout(Duration::from_secs(5), conn.accept_bidi_stream())
                    .await
                {
                    if let Ok(mut stream) = stream {
                        // Read frame from client
                        let mut buf = vec![0u8; 1024];
                        if let Ok(n) = stream.recv.read(&mut buf).await {
                            if n > 0 {
                                buf.truncate(n);
                                // Send echo back
                                let _ = stream.send.write_all(&buf).await;
                                let _ = stream.send.finish().await;
                                return Ok::<_, Box<dyn std::error::Error>>(buf);
                            }
                        }
                    }
                }
            }
        }
        Err("server timeout".into())
    });

    // Client: connect and send frame on bidirectional stream
    let client_tls = create_test_client_tls();
    let client = QuicClient::new(client_tls)?;
    let mut conn = timeout(
        Duration::from_secs(5),
        client.connect(listen_addr, "localhost"),
    )
    .await??;

    let mut stream = conn.open_bidi_stream().await?;

    let test_payload = b"Echo test payload".to_vec();
    let frame = FrameBytes {
        header: FrameHeader {
            frame_type: FrameType::RpcMetadata,
            request_id: 99,
            session_id: 200,
            tx_id: None,
            payload_length: test_payload.len() as u64,
            flags: 0,
            header_crc: FRAME_HEADER_CRC_UNCHECKED,
        },
        payload: test_payload.clone(),
    };

    let encoded = FrameCodec::encode(&frame)?;
    stream.write_all(&encoded).await?;
    stream.finish().await?;

    // Read echo response
    let mut response_buf = vec![0u8; 1024];
    let n = stream.recv.read(&mut response_buf).await?;
    response_buf.truncate(n);

    assert_eq!(
        response_buf, encoded,
        "Server should echo back exact frame bytes"
    );

    // Wait for server
    let echo_result = timeout(Duration::from_secs(5), server_handle).await???;
    assert_eq!(
        echo_result, encoded,
        "Server should have received frame bytes"
    );

    Ok(())
}

// ============================================================================
// Test: Multiple Sequential Frames on Single Stream
// ============================================================================

#[tokio::test]
async fn test_multiple_sequential_frames() -> andromeda_core::AndromedaResult<()> {
    let server_addr = allocate_test_address();
    let server_tls = create_test_server_tls()?;
    let server = QuicServer::new(server_addr, server_tls)?;
    let listen_addr = server.local_addr();

    let frame_count = Arc::new(AtomicU32::new(0));
    let frame_count_clone = frame_count.clone();

    // Server: accept and read frames
    let server_handle = tokio::spawn(async move {
        if let Ok(mut conn) = timeout(Duration::from_secs(5), server.accept_connection()).await {
            if let Ok(mut conn) = conn {
                if let Ok(mut stream) = timeout(Duration::from_secs(5), conn.accept_bidi_stream())
                    .await
                {
                    if let Ok(mut stream) = stream {
                        let mut buf = vec![0u8; 4096];
                        loop {
                            match stream.recv.read(&mut buf).await {
                                Ok(0) => break, // EOF
                                Ok(n) => {
                                    frame_count_clone.fetch_add(1, Ordering::SeqCst);
                                    buf.truncate(n);
                                    // Echo back
                                    let _ = stream.send.write_all(&buf).await;
                                }
                                Err(_) => break,
                            }
                        }
                    }
                }
            }
        }
    });

    // Client: send multiple frames
    let client_tls = create_test_client_tls();
    let client = QuicClient::new(client_tls)?;
    let mut conn = timeout(
        Duration::from_secs(5),
        client.connect(listen_addr, "localhost"),
    )
    .await??;

    let mut stream = conn.open_bidi_stream().await?;

    for i in 0..5 {
        let payload = format!("Frame {}", i).into_bytes();
        let frame = FrameBytes {
            header: FrameHeader {
                frame_type: FrameType::RpcMetadata,
                request_id: (i as u64) + 1,
                session_id: 300,
                tx_id: None,
                payload_length: payload.len() as u64,
                flags: 0,
                header_crc: FRAME_HEADER_CRC_UNCHECKED,
            },
            payload,
        };

        let encoded = FrameCodec::encode(&frame)?;
        stream.write_all(&encoded).await?;
    }

    stream.finish().await?;

    // Wait for server processing
    timeout(Duration::from_secs(5), server_handle).await?;

    assert_eq!(
        frame_count.load(Ordering::SeqCst),
        5,
        "Server should have received all 5 frames"
    );

    Ok(())
}

// ============================================================================
// Test: Large Payload Frame
// ============================================================================

#[tokio::test]
async fn test_large_payload_frame() -> andromeda_core::AndromedaResult<()> {
    let server_addr = allocate_test_address();
    let server_tls = create_test_server_tls()?;
    let server = QuicServer::new(server_addr, server_tls)?;
    let listen_addr = server.local_addr();

    let payload_size = 65536; // 64KB payload

    // Server: accept and echo
    let server_handle = tokio::spawn(async move {
        if let Ok(mut conn) = timeout(Duration::from_secs(10), server.accept_connection()).await {
            if let Ok(mut conn) = conn {
                if let Ok(mut stream) = timeout(Duration::from_secs(10), conn.accept_bidi_stream())
                    .await
                {
                    if let Ok(mut stream) = stream {
                        let mut buf = vec![0u8; payload_size + 100];
                        if let Ok(n) = stream.recv.read(&mut buf).await {
                            if n > 0 {
                                buf.truncate(n);
                                let _ = stream.send.write_all(&buf).await;
                                let _ = stream.send.finish().await;
                                return Ok::<_, Box<dyn std::error::Error>>(n);
                            }
                        }
                    }
                }
            }
        }
        Err("server timeout".into())
    });

    // Client: send large frame
    let client_tls = create_test_client_tls();
    let client = QuicClient::new(client_tls)?;
    let mut conn = timeout(
        Duration::from_secs(10),
        client.connect(listen_addr, "localhost"),
    )
    .await??;

    let mut stream = conn.open_bidi_stream().await?;

    let large_payload = vec![0xAB; payload_size];
    let frame = FrameBytes {
        header: FrameHeader {
            frame_type: FrameType::RpcMetadata,
            request_id: 500,
            session_id: 400,
            tx_id: None,
            payload_length: large_payload.len() as u64,
            flags: 0,
            header_crc: FRAME_HEADER_CRC_UNCHECKED,
        },
        payload: large_payload,
    };

    let encoded = FrameCodec::encode(&frame)?;
    stream.write_all(&encoded).await?;
    stream.finish().await?;

    // Read echo
    let mut response_buf = vec![0u8; encoded.len() + 100];
    let n = stream.recv.read(&mut response_buf).await?;
    response_buf.truncate(n);

    assert_eq!(
        response_buf, encoded,
        "Server should echo back large frame exactly"
    );

    let bytes_received = timeout(Duration::from_secs(10), server_handle).await???;
    assert_eq!(
        bytes_received, encoded.len(),
        "Server should have received all bytes"
    );

    Ok(())
}
