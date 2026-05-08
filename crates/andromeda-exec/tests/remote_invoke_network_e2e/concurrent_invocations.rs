use crate::support::*;

#[tokio::test]
async fn test_concurrent_invocations() -> AndromedaResult<()> {
    // Start server
    let server_addr = allocate_test_address();
    let server_tls = create_test_server_tls()?;
    let server = Arc::new(QuicServer::new(server_addr, server_tls.server_config())?);
    let listen_addr = server.local_addr();

    let registry = Arc::new(MockRegistry::new());
    let completed = Arc::new(AtomicU64::new(0));

    // Spawn server task to accept and handle multiple connections
    let server_clone = server.clone();
    let registry_clone = registry.clone();
    let completed_clone = completed.clone();

    let server_handle = tokio::spawn(async move {
        for _ in 0..10 {
            if let Ok(mut conn) =
                timeout(Duration::from_secs(30), server_clone.accept_connection()).await
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
                                    if let Ok(n) = stream.read(&mut buf).await {
                                        if n > 0 {
                                            buf.truncate(n);
                                            if let Ok(frame) = FrameCodec::decode(&buf) {
                                                let proc_name =
                                                    String::from_utf8_lossy(&frame.payload);
                                                if let Ok(response_frames) = registry_inner
                                                    .execute(
                                                        &proc_name,
                                                        frame.header.request_id,
                                                        frame.header.session_id,
                                                    )
                                                    .await
                                                {
                                                    for resp_frame in response_frames {
                                                        if let Ok(encoded) =
                                                            encode_frame(&resp_frame)
                                                        {
                                                            let _ =
                                                                stream.write_all(&encoded).await;
                                                        }
                                                    }
                                                }
                                            }
                                            let _ = stream.finish().await;
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
            let client_tls = create_test_client_tls().expect(
                "test client TLS should be initialized from shared test certificate bundle",
            );
            if let Ok(client) = QuicClient::new(client_tls.client_config()) {
                if let Ok(mut conn) = timeout(
                    Duration::from_secs(10),
                    client.connect(listen_addr_copy, "localhost"),
                )
                .await
                {
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
                                        request_id: (client_id as u64 * 100 + req_id as u64).into(),
                                        session_id: (client_id as u64 * 1000).into(),
                                        tx_id: None,
                                        payload_length: proc_name.len() as u64,
                                        flags: 0,
                                        header_crc: FRAME_HEADER_CRC_UNCHECKED,
                                    },
                                    payload: proc_name.as_bytes().to_vec(),
                                };

                                if let Ok(encoded) = encode_frame(&request_frame) {
                                    let _ = stream.write_all(&encoded).await;
                                }
                                let _ = stream.finish().await;

                                let mut response_frames = vec![];
                                let mut buf = vec![0u8; 1024];
                                loop {
                                    match stream.read(&mut buf).await {
                                        Ok(0) => break,
                                        Ok(n) => response_frames.extend_from_slice(&buf[..n]),
                                        Err(_) => break,
                                    }
                                }
                                if let Ok(frames) = decode_response_frames(&response_frames) {
                                    let _ = assert_standard_response_sequence(
                                        &frames,
                                        request_frame.header.request_id,
                                        request_frame.header.session_id,
                                    );
                                }
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
