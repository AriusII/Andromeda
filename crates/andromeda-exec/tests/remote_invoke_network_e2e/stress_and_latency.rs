use crate::support::*;

#[tokio::test]
async fn test_stress_rapid_procedures() -> AndromedaResult<()> {
    // Start server
    let server_addr = allocate_test_address();
    let server_tls = create_test_server_tls()?;
    let server = QuicServer::new(server_addr, server_tls.server_config())?;
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
                            if let Ok(mut stream) =
                                timeout(Duration::from_secs(5), conn.accept_bidi_stream()).await
                            {
                                if let Ok(mut stream) = stream {
                                    let mut buf = vec![0u8; 512];
                                    if let Ok(n) = stream.read(&mut buf).await {
                                        if n > 0 {
                                            buf.truncate(n);
                                    if let Ok(frame) = FrameCodec::decode(&buf) {
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
                                                if let Ok(encoded) = encode_frame(&resp_frame) {
                                                    let _ = stream.write_all(&encoded).await;
                                                }
                                            }
                                        }
                                    }
                                    let _ = stream.finish().await;
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
    let client_tls = create_test_client_tls()?;
    let client = QuicClient::new(client_tls.client_config())?;
    let mut conn = timeout(
        Duration::from_secs(10),
        client.connect(listen_addr, "localhost"),
    )
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
                    request_id: req_id.into(),
                    session_id: 5000.into(),
                    tx_id: None,
                    payload_length: proc_name.len() as u64,
                    flags: 0,
                    header_crc: FRAME_HEADER_CRC_UNCHECKED,
                },
                payload: proc_name.as_bytes().to_vec(),
            };

            let encoded = encode_frame(&request_frame)?;
            let _ = stream.write_all(&encoded).await;
            let _ = stream.finish().await;

            // Drain response asynchronously
            let mut buf = vec![0u8; 1024];
            let _ = stream.read(&mut buf).await;
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
#[tokio::test]
async fn test_latency_measurements() -> AndromedaResult<()> {
    // Start server
    let server_addr = allocate_test_address();
    let server_tls = create_test_server_tls()?;
    let server = QuicServer::new(server_addr, server_tls.server_config())?;
    let listen_addr = server.local_addr();

    let registry = Arc::new(MockRegistry::new());

    // Server task
    let registry_clone = registry.clone();
    let server_handle = tokio::spawn(async move {
        if let Ok(mut conn) = timeout(Duration::from_secs(30), server.accept_connection()).await {
            if let Ok(mut conn) = conn {
                for _ in 0..100 {
                            if let Ok(mut stream) =
                                timeout(Duration::from_secs(5), conn.accept_bidi_stream()).await
                            {
                                if let Ok(mut stream) = stream {
                                    let mut buf = vec![0u8; 512];
                                    if let Ok(n) = stream.read(&mut buf).await {
                                        if n > 0 {
                                            buf.truncate(n);
                                    if let Ok(frame) = FrameCodec::decode(&buf) {
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
                                                if let Ok(encoded) = encode_frame(&resp_frame) {
                                                    let _ = stream.write_all(&encoded).await;
                                                }
                                            }
                                        }
                                    }
                                    let _ = stream.finish().await;
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    // Client: measure latencies
    let client_tls = create_test_client_tls()?;
    let client = QuicClient::new(client_tls.client_config())?;
    let mut conn = timeout(
        Duration::from_secs(10),
        client.connect(listen_addr, "localhost"),
    )
    .await??;

    let mut latencies = vec![];

    for req_id in 0..100 {
        let start = Instant::now();

        let mut stream = conn.open_bidi_stream().await?;

        let proc_name = "system.version";
        let request_frame = FrameBytes {
            header: FrameHeader {
                frame_type: FrameType::RpcExecuteRequest,
                request_id: req_id.into(),
                session_id: 6000.into(),
                tx_id: None,
                payload_length: proc_name.len() as u64,
                flags: 0,
                header_crc: FRAME_HEADER_CRC_UNCHECKED,
            },
            payload: proc_name.as_bytes().to_vec(),
        };

        let encoded = encode_frame(&request_frame)?;
        stream.write_all(&encoded).await?;
        stream.finish().await?;

        let mut buf = vec![0u8; 512];
        let _ = stream.read(&mut buf).await;

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
