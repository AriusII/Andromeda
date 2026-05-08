use crate::support::*;

#[tokio::test]
async fn test_single_invocation_e2e() -> AndromedaResult<()> {
    // Start server
    let server_addr = allocate_test_address();
    let server_tls = create_test_server_tls()?;
    let server = QuicServer::new(server_addr, server_tls.server_config())?;
    let listen_addr = server.local_addr();

    let registry = MockRegistry::new();

    // Server task: accept connection and handle a single request.
    let server_handle = tokio::spawn(async move {
        let mut conn = with_timeout(Duration::from_secs(10), server.accept_connection()).await?;
        let mut stream = with_timeout(Duration::from_secs(10), conn.accept_bidi_stream()).await?;

        // Read request frame.
        let mut buf = vec![0u8; 512];
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Err(andromeda_core::AndromedaError::new(
                andromeda_core::AndromedaErrorKind::Transport,
                "empty request frame",
            ));
        }
        buf.truncate(n);

        let frame = FrameCodec::decode(&buf)?;
        let proc_name = String::from_utf8_lossy(&frame.payload);

        let response_frames = registry
            .execute(
                &proc_name,
                frame.header.request_id,
                frame.header.session_id,
            )
            .await?;

        for resp_frame in response_frames {
            let encoded = encode_frame(&resp_frame)?;
            stream.write_all(&encoded).await?;
        }
        stream.finish().await?;

        Ok::<_, andromeda_core::AndromedaError>(())
    });

    // Client: connect and invoke procedure
    let client_tls = create_test_client_tls()?;
    let client = QuicClient::new(client_tls.client_config())?;
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
            request_id: 1001.into(),
            session_id: 2001.into(),
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

    // Receive response frames
    let mut response_frames = vec![];
    let mut buf = vec![0u8; 1024];

    loop {
        match stream.read(&mut buf).await {
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
    let first_frame = FrameCodec::decode(&response_frames)?;
    assert_eq!(
        first_frame.header.frame_type,
        FrameType::RpcMetadata,
        "First response frame should be metadata"
    );

    // Wait for server.
    join_with_timeout(Duration::from_secs(10), server_handle).await?;

    Ok(())
}

