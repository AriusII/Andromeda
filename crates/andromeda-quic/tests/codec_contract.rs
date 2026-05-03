use andromeda_core::{AndromedaErrorKind, RequestId, SessionId, TransactionId};
use andromeda_quic::{
    dispatch_frame, expected_stream_role, validate_transport_surface, BackpressureReason,
    BackpressureSignal, DispatchPolicy, FrameBytes, FrameCodec, FrameHeader,
    FrameType, StreamRole, TransportSurface, FRAME_CODEC_CRC_OFFSET, FRAME_CODEC_HEADER_LEN,
    FRAME_HEADER_CRC_UNCHECKED,
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
    let mut policy = DispatchPolicy::new(StreamRole::ResultUnidirectional);
    policy
        .dispatch(&frame(FrameType::RpcMetadata, b"meta".to_vec()))
        .unwrap();
    policy
        .dispatch(&frame(FrameType::RpcBatch, b"row".to_vec()))
        .unwrap();
    policy
        .dispatch(&frame(FrameType::RpcCompletion, Vec::new()))
        .unwrap();
    assert!(policy.finish().is_ok());

    let mut wrong_order = DispatchPolicy::new(StreamRole::ResultUnidirectional);
    assert_eq!(
        wrong_order
            .dispatch(&frame(FrameType::RpcBatch, b"row".to_vec()))
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
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
