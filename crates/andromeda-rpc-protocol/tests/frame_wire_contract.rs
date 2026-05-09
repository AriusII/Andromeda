use andromeda_error::AndromedaErrorKind;
use andromeda_rpc_protocol::{
    ERROR_FRAME_CODE, FRAME_CODEC_CRC_OFFSET, FRAME_CODEC_HEADER_LEN, FRAME_CODEC_VERSION,
    FRAME_TYPE_PAYLOAD_CODE_LOCKSTEP, FrameBytes, FrameCodec, FrameCodecEndian, FrameHeader,
    FrameType, MAX_FRAME_PAYLOAD_LENGTH, PayloadKindInvariants, RESERVED_FRAME_FLAGS_MASK,
    RPC_BATCH_FRAME_CODE, RPC_COMPLETION_FRAME_CODE, RPC_EXECUTE_REQUEST_FRAME_CODE,
    RPC_METADATA_FRAME_CODE, ResultStreamMetadataPolicy, StreamRole,
    validate_result_stream_sequence, validate_result_stream_sequence_with_metadata_policy,
};
use andromeda_types::{RequestId, SessionId, TransactionId};

const REQUEST_ID: u64 = 0x0102_0304_0506_0708;
const SESSION_ID: u64 = 0x1112_1314_1516_1718;
const TX_ID: u64 = 0x2122_2324_2526_2728;

fn header(frame_type: FrameType, payload_length: u64) -> FrameHeader {
    FrameHeader {
        frame_type,
        request_id: RequestId::new(REQUEST_ID),
        session_id: SessionId::new(SESSION_ID),
        tx_id: Some(TransactionId::new(TX_ID)),
        payload_length,
        flags: 0,
        header_crc: 0,
    }
}

fn frame(frame_type: FrameType, payload: impl Into<Vec<u8>>) -> FrameBytes {
    let payload = payload.into();
    FrameBytes {
        header: header(frame_type, payload.len() as u64),
        payload,
    }
}

fn encoded_execute_frame() -> Vec<u8> {
    FrameCodec::encode(&frame(FrameType::RpcExecuteRequest, b"body".to_vec()))
        .expect("representative RPC execute frame must encode")
}

#[test]
fn frame_header_wire_contract_is_fixed_width_and_network_big_endian() {
    let encoded = encoded_execute_frame();

    assert_eq!(FrameCodec::HEADER_LEN, 52);
    assert_eq!(FRAME_CODEC_HEADER_LEN, 52);
    assert_eq!(encoded.len(), FRAME_CODEC_HEADER_LEN + 4);
    assert_eq!(
        FrameCodec::ENDIAN,
        FrameCodecEndian::NetworkBigEndian,
        "RPC frame wire stays network-byte-order by explicit exception until the ADR/docs decision resolves the repository little-endian default"
    );

    // Deliberate RPC network-wire exception to repository little-endian
    // persistent-format defaults, pending the ADR/docs decision.
    assert_eq!(
        &encoded[0..2],
        &(FRAME_CODEC_HEADER_LEN as u16).to_be_bytes()
    );
    assert_eq!(&encoded[2..4], &FRAME_CODEC_VERSION.to_be_bytes());
    assert_eq!(
        &encoded[4..8],
        &RPC_EXECUTE_REQUEST_FRAME_CODE.to_be_bytes()
    );
    assert_eq!(&encoded[8..16], &REQUEST_ID.to_be_bytes());
    assert_eq!(&encoded[16..24], &SESSION_ID.to_be_bytes());
    assert_eq!(&encoded[24..32], &TX_ID.to_be_bytes());
    assert_eq!(encoded[32], 1);
    assert_eq!(&encoded[33..36], &[0, 0, 0]);
    assert_eq!(&encoded[36..44], &4_u64.to_be_bytes());
    assert_eq!(&encoded[44..48], &0_u32.to_be_bytes());

    assert_ne!(&encoded[8..16], &REQUEST_ID.to_le_bytes());
    assert_ne!(&encoded[16..24], &SESSION_ID.to_le_bytes());
    assert_ne!(&encoded[24..32], &TX_ID.to_le_bytes());
}

#[test]
fn frame_header_crc_offset_and_calculation_are_locked() {
    let encoded = encoded_execute_frame();
    let crc = read_u32(&encoded, FRAME_CODEC_CRC_OFFSET);

    assert_eq!(FRAME_CODEC_CRC_OFFSET, 48);
    assert_eq!(FRAME_CODEC_CRC_OFFSET + 4, FRAME_CODEC_HEADER_LEN);
    assert_ne!(crc, 0);
    assert_eq!(crc, crc32_with_encoded_crc_zeroed(&encoded));
    assert_eq!(
        FrameCodec::header_crc(&header(FrameType::RpcExecuteRequest, 4))
            .expect("static header fields are valid"),
        crc
    );
}

#[test]
fn reserved_flags_are_rejected_on_encode_and_decode() {
    let mut flagged = frame(FrameType::RpcExecuteRequest, b"body".to_vec());
    flagged.header.flags = RESERVED_FRAME_FLAGS_MASK;

    assert_eq!(
        FrameCodec::encode(&flagged).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );

    let mut encoded = encoded_execute_frame();
    encoded[44..48].copy_from_slice(&RESERVED_FRAME_FLAGS_MASK.to_be_bytes());
    rewrite_header_crc(&mut encoded);

    assert_eq!(
        FrameCodec::decode(&encoded).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn reserved_header_bytes_must_stay_zero_on_decode() {
    for reserved_offset in 33..36 {
        let mut encoded = encoded_execute_frame();
        encoded[reserved_offset] = 0xA5;
        rewrite_header_crc(&mut encoded);

        assert_eq!(
            FrameCodec::decode(&encoded).unwrap_err().kind(),
            AndromedaErrorKind::Protocol,
            "reserved header byte at offset {reserved_offset} must reject the frame"
        );
    }
}

#[test]
fn tx_id_present_marker_accepts_only_zero_or_one() {
    let mut encoded = encoded_execute_frame();
    encoded[32] = 2;
    rewrite_header_crc(&mut encoded);

    assert_eq!(
        FrameCodec::decode(&encoded).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn header_crc_mismatch_rejects_tampered_header() {
    let mut encoded = encoded_execute_frame();
    encoded[8] ^= 0xAA;

    assert_eq!(
        FrameCodec::decode(&encoded).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn payload_bounds_reject_oversize_declarations_and_truncation() {
    let mut oversized = encoded_execute_frame();
    oversized[36..44].copy_from_slice(&(MAX_FRAME_PAYLOAD_LENGTH + 1).to_be_bytes());
    rewrite_header_crc(&mut oversized);

    assert_eq!(
        FrameCodec::scan_one(&oversized).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );

    let mut truncated = encoded_execute_frame();
    truncated[36..44].copy_from_slice(&5_u64.to_be_bytes());
    rewrite_header_crc(&mut truncated);

    assert_eq!(
        FrameCodec::decode(&truncated).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn decode_rejects_trailing_bytes_while_scan_one_reports_first_frame_length() {
    let encoded = encoded_execute_frame();
    let mut with_trailing = encoded.clone();
    with_trailing.extend_from_slice(b"trail");

    let (_, consumed) =
        FrameCodec::scan_one(&with_trailing).expect("scan_one must read the first frame");
    assert_eq!(consumed, encoded.len());

    assert_eq!(
        FrameCodec::decode(&with_trailing).unwrap_err().kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn decoded_empty_rpc_batch_is_rejected_by_frame_validation() {
    let encoded = FrameCodec::encode(&frame(FrameType::RpcBatch, Vec::new()))
        .expect("wire decoder test frame must encode");
    let decoded = FrameCodec::decode(&encoded).expect("empty RpcBatch wire frame must decode");

    assert_eq!(decoded.header.frame_type, FrameType::RpcBatch);
    assert!(decoded.payload.is_empty());
    assert_eq!(
        decoded
            .validate(StreamRole::ResultUnidirectional)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn bounded_scan_accepts_the_limit_and_rejects_one_more_frame() {
    let encoded = FrameCodec::encode(&frame(FrameType::RpcCompletion, Vec::new()))
        .expect("empty completion frame must encode");
    let mut batch = Vec::with_capacity(encoded.len() * (FrameCodec::MAX_SCAN_FRAMES + 1));

    for _ in 0..FrameCodec::MAX_SCAN_FRAMES {
        batch.extend_from_slice(&encoded);
    }

    let frames = FrameCodec::scan_all(&batch).expect("scan at the limit must succeed");
    assert_eq!(frames.len(), FrameCodec::MAX_SCAN_FRAMES);

    batch.extend_from_slice(&encoded);
    assert_eq!(
        FrameCodec::scan_all(&batch).unwrap_err().kind(),
        AndromedaErrorKind::Resource
    );
}

#[test]
fn result_stream_requires_metadata_before_batch_payload() {
    let metadata = frame(FrameType::RpcMetadata, b"columns".to_vec());
    let batch = frame(FrameType::RpcBatch, b"row".to_vec());
    let completion = frame(FrameType::RpcCompletion, Vec::new());

    assert!(
        validate_result_stream_sequence(&[metadata.clone(), batch.clone(), completion.clone()])
            .is_ok()
    );
    assert!(metadata.validate(StreamRole::ResultUnidirectional).is_ok());
    assert!(batch.validate(StreamRole::ResultUnidirectional).is_ok());
    assert!(
        completion
            .validate(StreamRole::ResultUnidirectional)
            .is_ok()
    );

    assert_eq!(
        validate_result_stream_sequence(&[batch.clone(), metadata.clone(), completion.clone()])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
    assert_eq!(
        validate_result_stream_sequence(&[completion.clone(), metadata.clone()])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
}

#[test]
fn zero_row_completion_still_requires_metadata_policy() {
    let metadata = frame(FrameType::RpcMetadata, b"zero-row-policy".to_vec());
    let completion = frame(FrameType::RpcCompletion, Vec::new());
    let sequence = [metadata, completion];

    assert_eq!(
        validate_result_stream_sequence(&sequence)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );
    assert!(
        validate_result_stream_sequence_with_metadata_policy(
            &sequence,
            ResultStreamMetadataPolicy::ZeroRowCompletionAllowed,
        )
        .is_ok()
    );
}

#[test]
fn payload_kind_locked_range_includes_error_payload() {
    assert_eq!(*PayloadKindInvariants::LOCKED_RANGE.start(), 1);
    assert_eq!(*PayloadKindInvariants::LOCKED_RANGE.end(), 9);
    assert!(
        PayloadKindInvariants::LOCKED_RANGE.contains(&PayloadKindInvariants::PAYLOAD_ERROR),
        "PAYLOAD_ERROR=9 must be inside the locked payload-kind range"
    );
    assert_eq!(PayloadKindInvariants::PAYLOAD_ERROR, ERROR_FRAME_CODE);
    assert!(
        FRAME_TYPE_PAYLOAD_CODE_LOCKSTEP
            .iter()
            .any(|(frame_type, payload_code)| {
                *frame_type == FrameType::Error
                    && *payload_code == PayloadKindInvariants::PAYLOAD_ERROR
            })
    );

    for (frame_type, payload_code) in FRAME_TYPE_PAYLOAD_CODE_LOCKSTEP {
        assert_eq!(frame_type.wire_code(), *payload_code);
        assert!(PayloadKindInvariants::LOCKED_RANGE.contains(payload_code));
    }
}

#[test]
fn result_stream_frame_codes_remain_payload_kind_lockstep() {
    assert_eq!(FrameType::RpcMetadata.wire_code(), RPC_METADATA_FRAME_CODE);
    assert_eq!(FrameType::RpcBatch.wire_code(), RPC_BATCH_FRAME_CODE);
    assert_eq!(
        FrameType::RpcCompletion.wire_code(),
        RPC_COMPLETION_FRAME_CODE
    );

    for code in [
        RPC_METADATA_FRAME_CODE,
        RPC_BATCH_FRAME_CODE,
        RPC_COMPLETION_FRAME_CODE,
    ] {
        assert!(PayloadKindInvariants::LOCKED_RANGE.contains(&code));
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("u32 field slice must be fixed width"),
    )
}

fn rewrite_header_crc(encoded: &mut [u8]) {
    encoded[FRAME_CODEC_CRC_OFFSET..FRAME_CODEC_CRC_OFFSET + 4].fill(0);
    let crc = crc32(&encoded[..FRAME_CODEC_HEADER_LEN]);
    encoded[FRAME_CODEC_CRC_OFFSET..FRAME_CODEC_CRC_OFFSET + 4].copy_from_slice(&crc.to_be_bytes());
}

fn crc32_with_encoded_crc_zeroed(encoded: &[u8]) -> u32 {
    let mut header = [0_u8; FRAME_CODEC_HEADER_LEN];
    header.copy_from_slice(&encoded[..FRAME_CODEC_HEADER_LEN]);
    header[FRAME_CODEC_CRC_OFFSET..FRAME_CODEC_CRC_OFFSET + 4].fill(0);
    crc32(&header)
}

fn crc32(bytes: &[u8]) -> u32 {
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
