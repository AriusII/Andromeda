use andromeda_core::{
    AndromedaErrorKind, CatalogVersion, ContractHash, RequestId, SessionId, TransactionId,
};
use andromeda_proto::{encode_generated_message, generated};
use andromeda_quic::{
    BackpressureReason, BackpressureSignal, DispatchPolicy, FRAME_CODEC_CRC_OFFSET,
    FRAME_CODEC_HEADER_LEN, FRAME_HEADER_CRC_UNCHECKED, FrameBytes, FrameCodec, FrameHeader,
    FrameType, ResultStreamMetadataPolicy, StreamRole, TransportSurface, TypedResultStreamContext,
    dispatch_frame, expected_stream_role, validate_transport_surface,
};

fn header(frame_type: FrameType, payload_length: u64) -> FrameHeader {
    FrameHeader {
        frame_type,
        request_id: RequestId::new(101),
        session_id: SessionId::new(202),
        tx_id: Some(TransactionId::new(303)),
        payload_length,
        flags: 0,
        header_crc: FRAME_HEADER_CRC_UNCHECKED,
    }
}

fn frame(frame_type: FrameType, payload: impl Into<Vec<u8>>) -> FrameBytes {
    let payload = payload.into();
    FrameBytes {
        header: header(frame_type, payload.len() as u64),
        payload,
    }
}

fn hash(byte: u8) -> Vec<u8> {
    vec![byte; ContractHash::LEN]
}

fn typed_result_stream_context() -> TypedResultStreamContext {
    TypedResultStreamContext::new(
        RequestId::new(101),
        SessionId::new(202),
        Some(TransactionId::new(303)),
        ContractHash::from_slice(&hash(7)).unwrap(),
        CatalogVersion::new(42),
    )
}

fn envelope_payload(kind: generated::protocol::v1::PayloadKind, payload: Vec<u8>) -> Vec<u8> {
    encode_generated_message(&generated::protocol::v1::FrameEnvelope {
        protocol_version: Some(generated::protocol::v1::ProtocolVersion { major: 1, minor: 0 }),
        contract_hash: hash(7),
        catalog_version: 42,
        request_id: 101,
        session_id: 202,
        tx_id: Some(303),
        payload_kind: kind as i32,
        payload,
    })
}

fn result_metadata_frame() -> FrameBytes {
    frame(
        FrameType::RpcMetadata,
        envelope_payload(
            generated::protocol::v1::PayloadKind::RpcMetadata,
            encode_generated_message(&generated::protocol::v1::RpcMetadata {
                result_streams: Vec::new(),
                completion_policy: None,
            }),
        ),
    )
}

fn result_batch_frame() -> FrameBytes {
    frame(
        FrameType::RpcBatch,
        envelope_payload(
            generated::protocol::v1::PayloadKind::RpcBatch,
            encode_generated_message(&generated::protocol::v1::RpcBatch {
                result_name: "Inventory.ReserveStock.Reservation".to_string(),
                batch_index: 0,
                rows_emitted: 1,
                structured_payload: b"\x01".to_vec(),
                row_count_exact: Some(1),
                terminal_batch: true,
            }),
        ),
    )
}

fn result_completion_frame() -> FrameBytes {
    frame(
        FrameType::RpcCompletion,
        envelope_payload(
            generated::protocol::v1::PayloadKind::RpcCompletion,
            encode_generated_message(&generated::protocol::v1::RpcCompletion {
                status: generated::protocol::v1::rpc_completion::Status::Committed as i32,
                rows_affected: Some(1),
                tx_id: Some(303),
                request_id: Some(101),
                session_id: Some(202),
                trace_id: Some("trace".to_string()),
                transaction_outcome:
                    generated::protocol::v1::rpc_completion::TransactionOutcome::Committed as i32,
                durable_lsn: Some(1),
                result_row_counts: Vec::new(),
            }),
        ),
    )
}

#[test]
fn frame_codec_round_trips_binary_header_and_payload() {
    let frame = frame(FrameType::RpcExecuteRequest, b"exec".to_vec());
    let encoded = FrameCodec::encode(&frame).unwrap();

    assert_eq!(encoded.len(), FRAME_CODEC_HEADER_LEN + 4);
    assert_ne!(
        &encoded[FRAME_CODEC_CRC_OFFSET..FRAME_CODEC_CRC_OFFSET + 4],
        &[0, 0, 0, 0]
    );

    let decoded = FrameCodec::decode(&encoded).unwrap();
    assert_eq!(decoded.header.frame_type, FrameType::RpcExecuteRequest);
    assert_eq!(decoded.header.request_id, RequestId::new(101));
    assert_eq!(decoded.header.session_id, SessionId::new(202));
    assert_eq!(decoded.header.tx_id, Some(TransactionId::new(303)));
    assert_eq!(decoded.header.payload_length, 4);
    assert_eq!(decoded.payload, b"exec");
}

#[test]
fn frame_codec_rejects_corrupt_header_crc_and_unknown_type() {
    let mut corrupt =
        FrameCodec::encode(&frame(FrameType::RpcExecuteRequest, b"exec".to_vec())).unwrap();
    corrupt[8] ^= 0x01;
    assert_eq!(
        FrameCodec::decode(&corrupt).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );

    let mut unknown =
        FrameCodec::encode(&frame(FrameType::RpcExecuteRequest, b"exec".to_vec())).unwrap();
    unknown[4..8].copy_from_slice(&999_u32.to_be_bytes());
    restamp_header_crc(&mut unknown);
    assert_eq!(
        FrameCodec::decode(&unknown).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn frame_codec_rejects_truncated_payload_reserved_flags_and_trailing_bytes() {
    let mut truncated =
        FrameCodec::encode(&frame(FrameType::RpcExecuteRequest, b"exec".to_vec())).unwrap();
    truncated.pop();
    assert_eq!(
        FrameCodec::decode(&truncated).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );

    let mut reserved_flags =
        FrameCodec::encode(&frame(FrameType::RpcExecuteRequest, b"exec".to_vec())).unwrap();
    reserved_flags[47] = 1;
    restamp_header_crc(&mut reserved_flags);
    assert_eq!(
        FrameCodec::decode(&reserved_flags).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );

    let mut trailing =
        FrameCodec::encode(&frame(FrameType::RpcExecuteRequest, b"exec".to_vec())).unwrap();
    trailing.push(0);
    assert_eq!(
        FrameCodec::decode(&trailing).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn dispatcher_rejects_wrong_stream_role() {
    let frame = frame(FrameType::RpcExecuteRequest, b"exec".to_vec());

    assert_eq!(
        expected_stream_role(&frame),
        StreamRole::CommandBidirectional
    );
    assert!(dispatch_frame(&frame, StreamRole::CommandBidirectional).is_ok());
    assert_eq!(
        dispatch_frame(&frame, StreamRole::ResultUnidirectional)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn dispatcher_validates_result_stream_sequence() {
    let mut policy = DispatchPolicy::new_result_stream(typed_result_stream_context());
    policy.dispatch(&result_metadata_frame()).unwrap();
    policy.dispatch(&result_batch_frame()).unwrap();
    policy.dispatch(&result_completion_frame()).unwrap();
    assert!(policy.finish().is_ok());

    let mut wrong_order = DispatchPolicy::new_result_stream(typed_result_stream_context());
    assert_eq!(
        wrong_order
            .dispatch(&result_batch_frame())
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn dispatcher_allows_metadata_only_completion_only_with_explicit_policy() {
    let mut strict = DispatchPolicy::new_result_stream(typed_result_stream_context());
    strict.dispatch(&result_metadata_frame()).unwrap();
    assert_eq!(
        strict
            .dispatch(&result_completion_frame())
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );

    let mut zero_row = DispatchPolicy::new_result_stream_with_metadata_policy(
        typed_result_stream_context(),
        ResultStreamMetadataPolicy::ZeroRowCompletionAllowed,
    );
    zero_row.dispatch(&result_metadata_frame()).unwrap();
    zero_row.dispatch(&result_completion_frame()).unwrap();
    assert!(zero_row.finish().is_ok());
}

#[test]
fn telemetry_is_datagram_only() {
    let telemetry = frame(FrameType::TelemetrySoftSignal, b"soft".to_vec());
    assert!(validate_transport_surface(&telemetry, TransportSurface::Datagram).is_ok());
    assert_eq!(
        validate_transport_surface(
            &telemetry,
            TransportSurface::ReliableStream(StreamRole::TelemetryDatagram),
        )
        .unwrap_err()
        .kind(),
        AndromedaErrorKind::Protocol
    );

    let execute = frame(FrameType::RpcExecuteRequest, b"exec".to_vec());
    assert_eq!(
        validate_transport_surface(&execute, TransportSurface::Datagram)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn backpressure_diagnostic_policy_stays_bounded_and_request_scoped() {
    let signal = BackpressureSignal {
        reason: BackpressureReason::ExecutionQueueSaturated,
        request_id: Some(RequestId::new(101)),
        retry_after_millis: Some(500),
    };
    assert!(signal.validate_retry_policy().is_ok());

    let diagnostic = frame(FrameType::Error, b"backpressure".to_vec());
    assert!(dispatch_frame(&diagnostic, StreamRole::Diagnostic).is_ok());
    assert_eq!(
        dispatch_frame(&diagnostic, StreamRole::CommandBidirectional)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
}

fn restamp_header_crc(encoded: &mut [u8]) {
    encoded[FRAME_CODEC_CRC_OFFSET..FRAME_CODEC_CRC_OFFSET + 4].fill(0);
    let crc = test_crc32(&encoded[..FRAME_CODEC_HEADER_LEN]);
    encoded[FRAME_CODEC_CRC_OFFSET..FRAME_CODEC_CRC_OFFSET + 4].copy_from_slice(&crc.to_be_bytes());
}

fn test_crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;

    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }

    !crc
}
