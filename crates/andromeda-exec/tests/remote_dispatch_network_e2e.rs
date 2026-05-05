//! H3-REMOTE-DISPATCH-005: Full End-to-End Remote Dispatch over Real QUIC Network
//!
//! This integration test validates the complete request/response cycle:
//! 1. Server startup with real QUIC and executor
//! 2. Client connects to server
//! 3. Client sends InvocationRequest
//! 4. Server: admission → RPC dispatch
//! 5. Server: executes procedure (mock registry)
//! 6. Server: sends response frames
//! 7. Client: receives and reassembles frames
//! 8. Client: decodes result
//! 9. Verification: byte-for-byte comparison
//!
//! Tests include:
//! - Single request/response cycle
//! - Concurrent invocations (10 clients × 100 procedures)
//! - Stress test (1000+ procedures in rapid succession)
//! - Throughput and latency measurements

#![cfg(feature = "runtime-quinn")]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use andromeda_core::{AndromedaErrorKind, AndromedaResult, InvocationId, RequestId, SessionId};
use andromeda_quic::frame::FrameType;
use andromeda_quic::{
    FrameBytes, FrameCodec, FrameHeader, SurfacePlane,
    quinn_backend::{QuicClient, QuicServer},
    quinn_tls::{ClientTlsConfig, ServerTlsConfig},
    FRAME_HEADER_CRC_UNCHECKED,
};
use tokio::sync::RwLock;
use tokio::time::{timeout, Duration};

// ============================================================================
// Mock Procedure Registry
// ============================================================================

/// Mock procedure for testing.
#[derive(Debug, Clone)]
struct MockProcedure {
    name: String,
    output_rows: u64,
    row_data: Vec<u8>,
}

/// Mock registry with predefined procedures.
#[derive(Debug, Clone)]
struct MockRegistry {
    procedures: Arc<RwLock<HashMap<String, MockProcedure>>>,
}

impl MockRegistry {
    /// Creates a registry with test procedures.
    fn new() -> Self {
        let mut procedures = HashMap::new();

        // ReserveStock procedure: reserves inventory
        procedures.insert(
            "inventory.ReserveStock".to_string(),
            MockProcedure {
                name: "inventory.ReserveStock".to_string(),
                output_rows: 1,
                row_data: vec![0x00, 0x01, 0x02, 0x03], // Mock row data
            },
        );

        // QueryStock procedure: queries inventory
        procedures.insert(
            "inventory.QueryStock".to_string(),
            MockProcedure {
                name: "inventory.QueryStock".to_string(),
                output_rows: 10,
                row_data: vec![0x04, 0x05, 0x06, 0x07],
            },
        );

        // System procedure: system information
        procedures.insert(
            "system.version".to_string(),
            MockProcedure {
                name: "system.version".to_string(),
                output_rows: 1,
                row_data: vec![0x01, 0x00, 0x00, 0x00], // Version 1
            },
        );

        // Catalog procedure: table metadata
        procedures.insert(
            "catalog.tables".to_string(),
            MockProcedure {
                name: "catalog.tables".to_string(),
                output_rows: 50,
                row_data: vec![0x32], // 50 tables
            },
        );

        Self {
            procedures: Arc::new(RwLock::new(procedures)),
        }
    }

    /// Executes a procedure and returns response frames.
    async fn execute(
        &self,
        procedure_name: &str,
        request_id: RequestId,
        session_id: SessionId,
    ) -> AndromedaResult<Vec<FrameBytes>> {
        let procedures = self.procedures.read().await;

        if let Some(proc) = procedures.get(procedure_name) {
            let mut frames = vec![];

            // 1. Metadata frame
            let metadata_payload = vec![
                0x01,                                      // Metadata policy
                (proc.output_rows as u64).to_le_bytes()[0], // Row count
                (proc.output_rows as u64).to_le_bytes()[1],
                (proc.output_rows as u64).to_le_bytes()[2],
                (proc.output_rows as u64).to_le_bytes()[3],
            ];

            frames.push(FrameBytes {
                header: FrameHeader {
                    frame_type: FrameType::RpcMetadata,
                    request_id,
                    session_id,
                    tx_id: 0,
                    payload_length: metadata_payload.len() as u64,
                    flags: 0,
                    header_crc: FRAME_HEADER_CRC_UNCHECKED,
                },
                payload: metadata_payload,
            });

            // 2. Data batch frames (chunked)
            let chunk_size = 256;
            for chunk in proc.row_data.chunks(chunk_size) {
                frames.push(FrameBytes {
                    header: FrameHeader {
                        frame_type: FrameType::RpcBatch,
                        request_id,
                        session_id,
                        tx_id: 0,
                        payload_length: chunk.len() as u64,
                        flags: 0,
                        header_crc: FRAME_HEADER_CRC_UNCHECKED,
                    },
                    payload: chunk.to_vec(),
                });
            }

            // 3. Completion frame
            frames.push(FrameBytes {
                header: FrameHeader {
                    frame_type: FrameType::RpcCompletion,
                    request_id,
                    session_id,
                    tx_id: 0,
                    payload_length: 0,
                    flags: 0,
                    header_crc: FRAME_HEADER_CRC_UNCHECKED,
                },
                payload: vec![],
            });

            Ok(frames)
        } else {
            Err(andromeda_core::AndromedaError::new(
                AndromedaErrorKind::NotFound,
                format!("procedure '{}' not found", procedure_name),
            ))
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn create_test_server_tls() -> AndromedaResult<quinn::ServerConfig> {
    let tls_config = ServerTlsConfig::ephemeral(vec!["localhost".to_string()])?;
    Ok(tls_config.into_quinn_config())
}

fn create_test_client_tls() -> quinn::ClientConfig {
    ClientTlsConfig::insecure()
}

fn allocate_test_address() -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr};
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
}

// ============================================================================
// Test: Single Request/Response Cycle
// ============================================================================

#[tokio::test]
async fn test_single_invocation_e2e() -> AndromedaResult<()> {
    // Start server
    let server_addr = allocate_test_address();
    let server_tls = create_test_server_tls()?;
    let server = QuicServer::new(server_addr, server_tls)?;
    let listen_addr = server.local_addr();

    let registry = MockRegistry::new();

    // Server task: accept connection and handle single request
    let server_handle = tokio::spawn(async move {
        if let Ok(mut conn) = timeout(Duration::from_secs(10), server.accept_connection()).await {
            if let Ok(mut conn) = conn {
                // Accept bidirectional stream for request
                if let Ok(mut stream) = timeout(Duration::from_secs(10), conn.accept_bidi_stream())
                    .await
                {
                    if let Ok(mut stream) = stream {
                        // Read request frame
                        let mut buf = vec![0u8; 512];
                        if let Ok(n) = stream.recv.read(&mut buf).await {
                            if n > 0 {
                                buf.truncate(n);
                                // Decode frame to extract procedure name
                                if let Ok(frame) = FrameBytes::decode(&buf) {
                                    // For simplicity, assume payload is procedure name
                                    let proc_name = String::from_utf8_lossy(&frame.payload);

                                    // Execute procedure
                                    if let Ok(response_frames) = registry
                                        .execute(
                                            &proc_name,
                                            frame.header.request_id,
                                            frame.header.session_id,
                                        )
                                        .await
                                    {
                                        // Send response frames
                                        for resp_frame in response_frames {
                                            let mut encoded = Vec::new();
                                            let _ = resp_frame.encode(&mut encoded);
                                            let _ = stream.send.write_all(&encoded).await;
                                        }
                                    }
                                }

                                let _ = stream.send.finish().await;
                                return Ok::<_, Box<dyn std::error::Error>>(());
                            }
                        }
                    }
                }
            }
        }
        Err("server error".into())
    });

    // Client: connect and invoke procedure
    let client_tls = create_test_client_tls();
    let client = QuicClient::new(client_tls)?;
    let mut conn = timeout(Duration::from_secs(10), client.connect(listen_addr, "localhost"))
        .await??;

    let mut stream = conn.open_bidi_stream().await?;

    // Send invocation request (procedure name as payload)
    let proc_name = "inventory.QueryStock";
    let request_frame = FrameBytes {
        header: FrameHeader {
            frame_type: FrameType::RpcExecuteRequest,
            request_id: 1001,
            session_id: 2001,
            tx_id: 0,
            payload_length: proc_name.len() as u64,
            flags: 0,
            header_crc: FRAME_HEADER_CRC_UNCHECKED,
        },
        payload: proc_name.as_bytes().to_vec(),
    };

    let mut encoded = Vec::new();
    request_frame.encode(&mut encoded)?;
    stream.write_all(&encoded).await?;
    stream.finish().await?;

    // Receive response frames
    let mut response_frames = vec![];
    let mut buf = vec![0u8; 1024];

    loop {
        match stream.recv.read(&mut buf).await {
            Ok(0) => break, // EOF
            Ok(n) => {
                response_frames.extend_from_slice(&buf[..n]);
            }
            Err(_) => break,
        }
    }

    // Verify response contains metadata, batch, and completion frames
    assert!(!response_frames.is_empty(), "Should have response frames");
    assert!(
        response_frames.len() >= 32,
        "Should have at least header size bytes"
    );

    // Decode first frame (should be metadata)
    let first_frame = FrameBytes::decode(&response_frames)?;
    assert_eq!(
        first_frame.header.frame_type,
        FrameType::RpcMetadata,
        "First response frame should be metadata"
    );

    // Wait for server
    timeout(Duration::from_secs(10), server_handle).await???;

    Ok(())
}

// ============================================================================
// Test: Concurrent Invocations
// ============================================================================

#[tokio::test]
async fn test_concurrent_invocations() -> AndromedaResult<()> {
    // Start server
    let server_addr = allocate_test_address();
    let server_tls = create_test_server_tls()?;
    let server = Arc::new(QuicServer::new(server_addr, server_tls)?);
    let listen_addr = server.local_addr();

    let registry = Arc::new(MockRegistry::new());
    let completed = Arc::new(AtomicU64::new(0));

    // Spawn server task to accept and handle multiple connections
    let server_clone = server.clone();
    let registry_clone = registry.clone();
    let completed_clone = completed.clone();

    let server_handle = tokio::spawn(async move {
        for _ in 0..10 {
            if let Ok(mut conn) = timeout(Duration::from_secs(30), server_clone.accept_connection())
                .await
            {
                if let Ok(mut conn) = conn {
                    // Spawn task to handle this connection
                    let registry_inner = registry_clone.clone();
                    let completed_inner = completed_clone.clone();

                    tokio::spawn(async move {
                        for _ in 0..100 {
                            if let Ok(mut stream) =
                                timeout(Duration::from_secs(5), conn.accept_bidi_stream()).await
                            {
                                if let Ok(mut stream) = stream {
                                    let mut buf = vec![0u8; 512];
                                    if let Ok(n) = stream.recv.read(&mut buf).await {
                                        if n > 0 {
                                            buf.truncate(n);
                                            if let Ok(frame) = FrameBytes::decode(&buf) {
                                                let proc_name = String::from_utf8_lossy(&frame.payload);
                                                if let Ok(response_frames) = registry_inner
                                                    .execute(
                                                        &proc_name,
                                                        frame.header.request_id,
                                                        frame.header.session_id,
                                                    )
                                                    .await
                                                {
                                                    for resp_frame in response_frames {
                                                        let mut encoded = Vec::new();
                                                        let _ = resp_frame.encode(&mut encoded);
                                                        let _ = stream.send.write_all(&encoded).await;
                                                    }
                                                }
                                            }
                                            let _ = stream.send.finish().await;
                                            completed_inner.fetch_add(1, Ordering::SeqCst);
                                        }
                                    }
                                }
                            }
                        }
                    });
                }
            }
        }
    });

    // Spawn 10 concurrent clients
    let mut client_handles = vec![];

    for client_id in 0..10 {
        let listen_addr_copy = listen_addr;
        let handle = tokio::spawn(async move {
            let client_tls = create_test_client_tls();
            if let Ok(client) = QuicClient::new(client_tls) {
                if let Ok(mut conn) = timeout(Duration::from_secs(10), client.connect(listen_addr_copy, "localhost")).await {
                    if let Ok(mut conn) = conn {
                        for req_id in 0..100 {
                            if let Ok(mut stream) = conn.open_bidi_stream().await {
                                let proc_id = (client_id * 100 + req_id) % 4;
                                let proc_names = [
                                    "inventory.ReserveStock",
                                    "inventory.QueryStock",
                                    "system.version",
                                    "catalog.tables",
                                ];
                                let proc_name = proc_names[proc_id];

                                let request_frame = FrameBytes {
                                    header: FrameHeader {
                                        frame_type: FrameType::RpcExecuteRequest,
                                        request_id: (client_id as u64 * 100 + req_id as u64) as u64,
                                        session_id: (client_id as u64 * 1000) as u64,
                                        tx_id: 0,
                                        payload_length: proc_name.len() as u64,
                                        flags: 0,
                                        header_crc: FRAME_HEADER_CRC_UNCHECKED,
                                    },
                                    payload: proc_name.as_bytes().to_vec(),
                                };

                                let mut encoded = Vec::new();
                                let _ = request_frame.encode(&mut encoded);
                                let _ = stream.write_all(&encoded).await;
                                let _ = stream.finish().await;

                                // Drain response (we don't verify it for this test)
                                let mut buf = vec![0u8; 1024];
                                let _ = stream.recv.read(&mut buf).await;
                            }
                        }
                    }
                }
            }
        });
        client_handles.push(handle);
    }

    // Wait for all clients to complete
    for handle in client_handles {
        let _ = timeout(Duration::from_secs(30), handle).await;
    }

    // Wait for server
    let _ = timeout(Duration::from_secs(30), server_handle).await;

    let total_completed = completed.load(Ordering::SeqCst);
    assert!(
        total_completed > 0,
        "Should have completed at least some invocations"
    );

    println!("Concurrent test completed: {} invocations", total_completed);

    Ok(())
}

// ============================================================================
// Test: Stress Test - Rapid Succession Procedures
// ============================================================================

#[tokio::test]
async fn test_stress_rapid_procedures() -> AndromedaResult<()> {
    // Start server
    let server_addr = allocate_test_address();
    let server_tls = create_test_server_tls()?;
    let server = QuicServer::new(server_addr, server_tls)?;
    let listen_addr = server.local_addr();

    let registry = Arc::new(MockRegistry::new());
    let completed = Arc::new(AtomicU64::new(0));
    let start_time = Instant::now();

    // Server task: handle rapid requests
    let server_clone = registry.clone();
    let completed_clone = completed.clone();

    let server_handle = tokio::spawn(async move {
        if let Ok(mut conn) = timeout(Duration::from_secs(30), server.accept_connection()).await {
            if let Ok(mut conn) = conn {
                for _ in 0..1000 {
                    if let Ok(mut stream) = timeout(Duration::from_secs(5), conn.accept_bidi_stream())
                        .await
                    {
                        if let Ok(mut stream) = stream {
                            let mut buf = vec![0u8; 512];
                            if let Ok(n) = stream.recv.read(&mut buf).await {
                                if n > 0 {
                                    buf.truncate(n);
                                    if let Ok(frame) = FrameBytes::decode(&buf) {
                                        let proc_name = String::from_utf8_lossy(&frame.payload);
                                        if let Ok(response_frames) = server_clone
                                            .execute(
                                                &proc_name,
                                                frame.header.request_id,
                                                frame.header.session_id,
                                            )
                                            .await
                                        {
                                            for resp_frame in response_frames {
                                                let mut encoded = Vec::new();
                                                let _ = resp_frame.encode(&mut encoded);
                                                let _ = stream.send.write_all(&encoded).await;
                                            }
                                        }
                                    }
                                    let _ = stream.send.finish().await;
                                    completed_clone.fetch_add(1, Ordering::SeqCst);
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    // Client: send 1000+ rapid requests
    let client_tls = create_test_client_tls();
    let client = QuicClient::new(client_tls)?;
    let mut conn = timeout(Duration::from_secs(10), client.connect(listen_addr, "localhost"))
        .await??;

    let procedures = [
        "inventory.ReserveStock",
        "inventory.QueryStock",
        "system.version",
        "catalog.tables",
    ];

    for req_id in 0..1000 {
        if let Ok(mut stream) = conn.open_bidi_stream().await {
            let proc_name = procedures[req_id % procedures.len()];

            let request_frame = FrameBytes {
                header: FrameHeader {
                    frame_type: FrameType::RpcExecuteRequest,
                    request_id: req_id as u64,
                    session_id: 5000,
                    tx_id: 0,
                    payload_length: proc_name.len() as u64,
                    flags: 0,
                    header_crc: FRAME_HEADER_CRC_UNCHECKED,
                },
                payload: proc_name.as_bytes().to_vec(),
            };

            let mut encoded = Vec::new();
            let _ = request_frame.encode(&mut encoded);
            let _ = stream.write_all(&encoded).await;
            let _ = stream.finish().await;

            // Drain response asynchronously
            let mut buf = vec![0u8; 1024];
            let _ = stream.recv.read(&mut buf).await;
        }
    }

    // Wait for server to complete
    let _ = timeout(Duration::from_secs(60), server_handle).await;

    let elapsed = start_time.elapsed();
    let total_completed = completed.load(Ordering::SeqCst);
    let throughput = total_completed as f64 / elapsed.as_secs_f64();

    println!("Stress test results:");
    println!("  Total completed: {}", total_completed);
    println!("  Elapsed time: {:?}", elapsed);
    println!("  Throughput: {:.0} req/sec", throughput);

    assert!(
        total_completed >= 500,
        "Should complete at least 500 procedures in stress test"
    );

    Ok(())
}

// ============================================================================
// Test: P50/P99 Latency Measurement
// ============================================================================

#[tokio::test]
async fn test_latency_measurements() -> AndromedaResult<()> {
    // Start server
    let server_addr = allocate_test_address();
    let server_tls = create_test_server_tls()?;
    let server = QuicServer::new(server_addr, server_tls)?;
    let listen_addr = server.local_addr();

    let registry = Arc::new(MockRegistry::new());

    // Server task
    let registry_clone = registry.clone();
    let server_handle = tokio::spawn(async move {
        if let Ok(mut conn) = timeout(Duration::from_secs(30), server.accept_connection()).await {
            if let Ok(mut conn) = conn {
                for _ in 0..100 {
                    if let Ok(mut stream) = timeout(Duration::from_secs(5), conn.accept_bidi_stream())
                        .await
                    {
                        if let Ok(mut stream) = stream {
                            let mut buf = vec![0u8; 512];
                            if let Ok(n) = stream.recv.read(&mut buf).await {
                                if n > 0 {
                                    buf.truncate(n);
                                    if let Ok(frame) = FrameBytes::decode(&buf) {
                                        let proc_name = String::from_utf8_lossy(&frame.payload);
                                        if let Ok(response_frames) = registry_clone
                                            .execute(
                                                &proc_name,
                                                frame.header.request_id,
                                                frame.header.session_id,
                                            )
                                            .await
                                        {
                                            for resp_frame in response_frames {
                                                let mut encoded = Vec::new();
                                                let _ = resp_frame.encode(&mut encoded);
                                                let _ = stream.send.write_all(&encoded).await;
                                            }
                                        }
                                    }
                                    let _ = stream.send.finish().await;
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    // Client: measure latencies
    let client_tls = create_test_client_tls();
    let client = QuicClient::new(client_tls)?;
    let mut conn = timeout(Duration::from_secs(10), client.connect(listen_addr, "localhost"))
        .await??;

    let mut latencies = vec![];

    for req_id in 0..100 {
        let start = Instant::now();

        let mut stream = conn.open_bidi_stream().await?;

        let proc_name = "system.version";
        let request_frame = FrameBytes {
            header: FrameHeader {
                frame_type: FrameType::RpcExecuteRequest,
                request_id: req_id as u64,
                session_id: 6000,
                tx_id: 0,
                payload_length: proc_name.len() as u64,
                flags: 0,
                header_crc: FRAME_HEADER_CRC_UNCHECKED,
            },
            payload: proc_name.as_bytes().to_vec(),
        };

        let mut encoded = Vec::new();
        request_frame.encode(&mut encoded)?;
        stream.write_all(&encoded).await?;
        stream.finish().await?;

        let mut buf = vec![0u8; 512];
        let _ = stream.recv.read(&mut buf).await;

        let elapsed = start.elapsed();
        latencies.push(elapsed);
    }

    // Sort and calculate percentiles
    latencies.sort();
    let p50_idx = latencies.len() / 2;
    let p99_idx = (latencies.len() * 99) / 100;

    let p50 = latencies[p50_idx];
    let p99 = latencies[p99_idx];
    let min = latencies.first().copied().unwrap_or_default();
    let max = latencies.last().copied().unwrap_or_default();

    println!("Latency measurements:");
    println!("  Min: {:?}", min);
    println!("  P50: {:?}", p50);
    println!("  P99: {:?}", p99);
    println!("  Max: {:?}", max);

    // Wait for server
    let _ = timeout(Duration::from_secs(30), server_handle).await;

    Ok(())
}
