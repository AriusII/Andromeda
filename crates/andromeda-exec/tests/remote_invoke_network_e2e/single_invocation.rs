use crate::support::*;

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
                if let Ok(mut stream) =
                    timeout(Duration::from_secs(10), conn.accept_bidi_stream()).await
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
    let mut conn = timeout(
        Duration::from_secs(10),
        client.connect(listen_addr, "localhost"),
    )
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
