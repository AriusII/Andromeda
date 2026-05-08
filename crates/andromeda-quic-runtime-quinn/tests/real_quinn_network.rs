//! Real Quinn network integration tests.
//!
//! This test suite validates real QUIC transport with:
//! - Server listening and accepting connections
//! - Client connection establishment with TLS
//! - Frame send/receive over QUIC streams
//! - Certificate identity extraction
//! - Concurrent connections
//! - End-to-end frame exchange

#![cfg(feature = "insecure-test-tls")]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use andromeda_core::AndromedaResult;
use andromeda_core::SurfaceScope;
use andromeda_quic::frame::{FRAME_HEADER_CRC_UNCHECKED, FrameType};
use andromeda_quic::{FrameBytes, FrameCodec, FrameHeader, SurfacePlane};
use andromeda_quic_runtime_quinn::{
    quinn_backend::{BidiStream, QuicClient, QuicServer, QuinnRuntimeSurface},
    quinn_tls::MutualTlsTestConfig,
};
use tokio::{sync::oneshot, task::JoinHandle, time::timeout};

fn create_test_tls() -> AndromedaResult<MutualTlsTestConfig> {
    MutualTlsTestConfig::ephemeral(vec!["localhost".to_string()])
}

fn test_runtime_surface() -> QuinnRuntimeSurface {
    QuinnRuntimeSurface::for_plane(SurfacePlane::Application)
}

/// Allocates an ephemeral local socket address.
fn allocate_test_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
}

fn transport_error(message: impl Into<String>) -> andromeda_core::AndromedaError {
    andromeda_core::AndromedaError::new(
        andromeda_core::AndromedaErrorKind::Transport,
        message.into(),
    )
}

async fn with_timeout<T>(
    duration: Duration,
    future: impl std::future::Future<Output = AndromedaResult<T>>,
) -> AndromedaResult<T> {
    timeout(duration, future)
        .await
        .map_err(|_| transport_error("operation timed out"))?
}

async fn join_with_timeout<T>(
    duration: Duration,
    handle: JoinHandle<AndromedaResult<T>>,
) -> AndromedaResult<T> {
    timeout(duration, handle)
        .await
        .map_err(|_| transport_error("join timed out"))?
        .map_err(|err| transport_error(format!("task join failed: {err}")))?
}

async fn read_bidi_to_end(stream: &mut BidiStream, max_bytes: usize) -> AndromedaResult<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buffer = vec![0_u8; 8_192.min(max_bytes.max(1))];

    loop {
        let n = stream.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        if bytes.len().saturating_add(n) > max_bytes {
            return Err(transport_error("test stream exceeded read limit"));
        }
        bytes.extend_from_slice(&buffer[..n]);
    }

    Ok(bytes)
}

#[tokio::test]
async fn test_server_startup_and_listen_address() -> AndromedaResult<()> {
    let addr = allocate_test_address();
    let tls = create_test_tls()?;

    let server = QuicServer::for_surface(addr, tls.server_config(), test_runtime_surface())?;
    let local_addr = server.local_addr();

    assert_eq!(server.runtime_surface(), test_runtime_surface());
    assert_eq!(local_addr.ip(), std::net::IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert_ne!(local_addr.port(), 0, "Port should be allocated");

    server.close(0, b"test complete");
    Ok(())
}

#[tokio::test]
async fn test_client_connection_tls_negotiation() -> AndromedaResult<()> {
    let server_addr = allocate_test_address();
    let tls = create_test_tls()?;
    let server = QuicServer::for_surface(server_addr, tls.server_config(), test_runtime_surface())?;
    let listen_addr = server.local_addr();

    let server_handle = tokio::spawn(async move {
        with_timeout(Duration::from_secs(5), server.accept_connection()).await
    });

    let client = QuicClient::for_surface(tls.client_config(), test_runtime_surface())?;
    assert_eq!(client.runtime_surface(), test_runtime_surface());

    let conn = with_timeout(
        Duration::from_secs(5),
        client.connect(listen_addr, "localhost"),
    )
    .await?;

    assert!(conn.is_open());
    assert_eq!(conn.remote_addr(), listen_addr);

    let server_conn = join_with_timeout(Duration::from_secs(5), server_handle).await?;
    assert!(server_conn.is_open());

    Ok(())
}

#[tokio::test]
async fn test_frame_echo_unidirectional() -> AndromedaResult<()> {
    let server_addr = allocate_test_address();
    let tls = create_test_tls()?;
    let server = QuicServer::for_surface(server_addr, tls.server_config(), test_runtime_surface())?;
    let listen_addr = server.local_addr();

    let server_handle = tokio::spawn(async move {
        let _conn = with_timeout(Duration::from_secs(5), server.accept_connection()).await?;
        Ok(())
    });

    let client = QuicClient::for_surface(tls.client_config(), test_runtime_surface())?;
    let mut conn = with_timeout(
        Duration::from_secs(5),
        client.connect(listen_addr, "localhost"),
    )
    .await?;

    let mut stream = conn.open_uni_stream().await?;

    let test_payload = b"Hello, QUIC!".to_vec();
    let frame = FrameBytes {
        header: FrameHeader {
            frame_type: FrameType::RpcMetadata,
            request_id: 42.into(),
            session_id: 100.into(),
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

    join_with_timeout(Duration::from_secs(5), server_handle).await?;

    Ok(())
}

#[tokio::test]
async fn test_peer_certificate_chain_is_exposed() -> AndromedaResult<()> {
    let server_addr = allocate_test_address();
    let tls = create_test_tls()?;
    let server = QuicServer::for_surface(server_addr, tls.server_config(), test_runtime_surface())?;
    let listen_addr = server.local_addr();

    let server_handle = tokio::spawn(async move {
        let conn = with_timeout(Duration::from_secs(5), server.accept_connection()).await?;
        Ok(conn.peer_certificate_chain().len())
    });

    let client = QuicClient::for_surface(tls.client_config(), test_runtime_surface())?;
    let conn = with_timeout(
        Duration::from_secs(5),
        client.connect(listen_addr, "localhost"),
    )
    .await?;

    let client_peer_certificates = conn.peer_certificate_chain();
    assert!(!client_peer_certificates.is_empty());
    assert!(!client_peer_certificates[0].is_empty());
    let client_identity = conn.certificate_identity();
    assert_eq!(client_identity.fingerprint().len(), 64);
    assert_eq!(client_identity.surface_scope(), SurfaceScope::Application);

    let server_peer_certificate_count =
        join_with_timeout(Duration::from_secs(5), server_handle).await?;
    assert_ne!(server_peer_certificate_count, 0);

    Ok(())
}

#[tokio::test]
async fn test_concurrent_connections() -> AndromedaResult<()> {
    let server_addr = allocate_test_address();
    let tls = create_test_tls()?;
    let server = Arc::new(QuicServer::for_surface(
        server_addr,
        tls.server_config(),
        test_runtime_surface(),
    )?);
    let listen_addr = server.local_addr();

    let accepted_count = Arc::new(AtomicU32::new(0));

    let server_clone = server.clone();
    let count_clone = accepted_count.clone();
    let accept_handle = tokio::spawn(async move {
        for _ in 0..10 {
            let _conn =
                with_timeout(Duration::from_secs(10), server_clone.accept_connection()).await?;
            count_clone.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    });

    let mut client_handles = vec![];
    for _ in 0..10 {
        let listen_addr_copy = listen_addr;
        let client_tls = tls.client_config();
        let handle = tokio::spawn(async move {
            let client = QuicClient::for_surface(client_tls, test_runtime_surface())?;
            let _conn = with_timeout(
                Duration::from_secs(5),
                client.connect(listen_addr_copy, "localhost"),
            )
            .await?;
            Ok(())
        });
        client_handles.push(handle);
    }

    for handle in client_handles {
        join_with_timeout(Duration::from_secs(10), handle).await?;
    }

    join_with_timeout(Duration::from_secs(10), accept_handle).await?;

    assert_eq!(
        accepted_count.load(Ordering::SeqCst),
        10,
        "Server should accept all 10 connections"
    );

    Ok(())
}

#[tokio::test]
async fn test_bidirectional_stream_communication() -> AndromedaResult<()> {
    let server_addr = allocate_test_address();
    let tls = create_test_tls()?;
    let server = QuicServer::for_surface(server_addr, tls.server_config(), test_runtime_surface())?;
    let listen_addr = server.local_addr();
    let (release_server, keep_server_alive) = oneshot::channel();

    let server_handle = tokio::spawn(async move {
        let mut conn = with_timeout(Duration::from_secs(5), server.accept_connection()).await?;
        let mut stream = with_timeout(Duration::from_secs(5), conn.accept_bidi_stream()).await?;
        let bytes = read_bidi_to_end(&mut stream, 16 * 1024).await?;
        if bytes.is_empty() {
            return Err(transport_error("server received empty stream"));
        }
        stream.write_all(&bytes).await?;
        stream.finish().await?;
        let _ = keep_server_alive.await;
        Ok(bytes)
    });

    let client = QuicClient::for_surface(tls.client_config(), test_runtime_surface())?;
    let mut conn = with_timeout(
        Duration::from_secs(5),
        client.connect(listen_addr, "localhost"),
    )
    .await?;

    let mut stream = conn.open_bidi_stream().await?;

    let test_payload = b"Echo test payload".to_vec();
    let frame = FrameBytes {
        header: FrameHeader {
            frame_type: FrameType::RpcMetadata,
            request_id: 99.into(),
            session_id: 200.into(),
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

    let response_buf = with_timeout(
        Duration::from_secs(5),
        read_bidi_to_end(&mut stream, 16 * 1024),
    )
    .await?;

    assert_eq!(
        response_buf, encoded,
        "Server should echo back exact frame bytes"
    );
    let _ = release_server.send(());

    let echo_result = join_with_timeout(Duration::from_secs(5), server_handle).await?;
    assert_eq!(
        echo_result, encoded,
        "Server should have received frame bytes"
    );

    Ok(())
}

#[tokio::test]
async fn test_multiple_sequential_frames() -> AndromedaResult<()> {
    let server_addr = allocate_test_address();
    let tls = create_test_tls()?;
    let server = QuicServer::for_surface(server_addr, tls.server_config(), test_runtime_surface())?;
    let listen_addr = server.local_addr();

    let frame_count = Arc::new(AtomicU32::new(0));
    let frame_count_clone = frame_count.clone();

    let server_handle = tokio::spawn(async move {
        let mut conn = with_timeout(Duration::from_secs(5), server.accept_connection()).await?;
        let mut stream = with_timeout(Duration::from_secs(5), conn.accept_bidi_stream()).await?;
        let bytes = read_bidi_to_end(&mut stream, 64 * 1024).await?;
        for _frame in FrameCodec::scan_all(&bytes)? {
            frame_count_clone.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    });

    let client = QuicClient::for_surface(tls.client_config(), test_runtime_surface())?;
    let mut conn = with_timeout(
        Duration::from_secs(5),
        client.connect(listen_addr, "localhost"),
    )
    .await?;

    let mut stream = conn.open_bidi_stream().await?;

    for i in 0..5 {
        let payload = format!("Frame {}", i).into_bytes();
        let frame = FrameBytes {
            header: FrameHeader {
                frame_type: FrameType::RpcMetadata,
                request_id: ((i as u64) + 1).into(),
                session_id: 300.into(),
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

    join_with_timeout(Duration::from_secs(5), server_handle).await?;

    assert_eq!(
        frame_count.load(Ordering::SeqCst),
        5,
        "Server should have received all 5 frames"
    );

    Ok(())
}

#[tokio::test]
async fn test_large_payload_frame() -> AndromedaResult<()> {
    let server_addr = allocate_test_address();
    let tls = create_test_tls()?;
    let server = QuicServer::for_surface(server_addr, tls.server_config(), test_runtime_surface())?;
    let listen_addr = server.local_addr();

    let payload_size = 65_536;
    let (release_server, keep_server_alive) = oneshot::channel();

    let server_handle = tokio::spawn(async move {
        let mut conn = with_timeout(Duration::from_secs(10), server.accept_connection()).await?;
        let mut stream = with_timeout(Duration::from_secs(10), conn.accept_bidi_stream()).await?;
        let bytes = read_bidi_to_end(&mut stream, payload_size + 1_024).await?;
        if bytes.is_empty() {
            return Err(transport_error("server received empty large payload"));
        }
        let n = bytes.len();
        stream.write_all(&bytes).await?;
        stream.finish().await?;
        let _ = keep_server_alive.await;
        Ok(n)
    });

    let client = QuicClient::for_surface(tls.client_config(), test_runtime_surface())?;
    let mut conn = with_timeout(
        Duration::from_secs(10),
        client.connect(listen_addr, "localhost"),
    )
    .await?;

    let mut stream = conn.open_bidi_stream().await?;

    let large_payload = vec![0xAB; payload_size];
    let frame = FrameBytes {
        header: FrameHeader {
            frame_type: FrameType::RpcMetadata,
            request_id: 500.into(),
            session_id: 400.into(),
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

    let response_buf = with_timeout(
        Duration::from_secs(10),
        read_bidi_to_end(&mut stream, encoded.len() + 1_024),
    )
    .await?;

    assert_eq!(
        response_buf, encoded,
        "Server should echo back large frame exactly"
    );
    let _ = release_server.send(());

    let bytes_received = join_with_timeout(Duration::from_secs(10), server_handle).await?;
    assert_eq!(
        bytes_received,
        encoded.len(),
        "Server should have received all bytes"
    );

    Ok(())
}
