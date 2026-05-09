use super::*;

/// Validates that FrameType enum values remain constant [1-9, 100].
///
/// Frame types are locked to prevent accidental mutations that would
/// corrupt the wire protocol frame discriminators.
#[test]
fn test_frame_type_discriminator_unchanged() {
    let locked_codes = [
        (FrameType::Hello, HELLO_FRAME_CODE, "Hello"),
        (FrameType::Auth, AUTH_FRAME_CODE, "Auth"),
        (
            FrameType::ContractRequest,
            CONTRACT_REQUEST_FRAME_CODE,
            "ContractRequest",
        ),
        (
            FrameType::ContractResponse,
            CONTRACT_RESPONSE_FRAME_CODE,
            "ContractResponse",
        ),
        (
            FrameType::RpcExecuteRequest,
            RPC_EXECUTE_REQUEST_FRAME_CODE,
            "RpcExecuteRequest",
        ),
        (
            FrameType::RpcMetadata,
            RPC_METADATA_FRAME_CODE,
            "RpcMetadata",
        ),
        (FrameType::RpcBatch, RPC_BATCH_FRAME_CODE, "RpcBatch"),
        (
            FrameType::RpcCompletion,
            RPC_COMPLETION_FRAME_CODE,
            "RpcCompletion",
        ),
        (FrameType::Error, ERROR_FRAME_CODE, "Error"),
        (
            FrameType::TelemetrySoftSignal,
            TELEMETRY_SOFT_SIGNAL_FRAME_CODE,
            "TelemetrySoftSignal",
        ),
    ];

    for (frame_type, expected_code, label) in locked_codes {
        assert_eq!(
            frame_type.wire_code(),
            expected_code,
            "{label} frame code changed"
        );
    }

    for code in 1..=9 {
        assert!(
            FrameType::try_from(code).is_ok(),
            "Frame type code {} should be valid",
            code
        );
    }

    assert!(
        FrameType::try_from(100).is_ok(),
        "Frame type code 100 should be valid"
    );

    assert!(
        FrameTypeInvariants::validate().is_ok(),
        "Frame type invariants validation failed"
    );
}

/// Validates that CRC position in frame header remains constant.
///
/// The CRC is positioned at a fixed offset for fast validation.
/// Any movement breaks frame validation.
#[test]
fn test_crc_position_stable() {
    assert_eq!(
        FRAME_CODEC_CRC_OFFSET, 48,
        "Frame CRC offset changed! Expected offset 48 (12 bytes from end of 52-byte header)"
    );
    assert_eq!(
        FRAME_CODEC_CRC_OFFSET + 4,
        52,
        "CRC offset + 4 doesn't equal header size"
    );

    let header = FrameHeader {
        frame_type: FrameType::Hello,
        request_id: RequestId::new(1),
        session_id: SessionId::new(2),
        tx_id: None,
        payload_length: 0,
        flags: 0,
        header_crc: 0xDEADBEEF,
    };

    assert_eq!(header.header_crc, 0xDEADBEEF, "CRC field not accessible");
}

/// Validates that MAX_FRAME_PAYLOAD_LENGTH remains 16 MiB.
///
/// The maximum payload size is a hard constraint for buffer management.
/// Any change affects memory allocation and flow control.
#[test]
fn test_frame_payload_max_size_locked() {
    assert_eq!(
        MAX_FRAME_PAYLOAD_LENGTH,
        16 * 1024 * 1024,
        "MAX_FRAME_PAYLOAD_LENGTH changed!"
    );

    let sixteen_mib = 16 * 1024 * 1024;
    assert_eq!(MAX_FRAME_PAYLOAD_LENGTH, sixteen_mib as u64);
}

/// Validates that bulk frame scans remain bounded.
///
/// This is a decoder resource contract, not a wire discriminator. The value can
/// change only with an explicit compatibility and backpressure review.
#[test]
fn test_frame_scan_batch_limit_locked() {
    assert_eq!(
        FrameCodec::MAX_SCAN_FRAMES,
        4_096,
        "FrameCodec::MAX_SCAN_FRAMES changed without protocol compatibility review"
    );
}

/// Demonstrates that field reordering would be caught by layout validation.
///
/// This test validates that our invariant checks would catch a hypothetical
/// accidental field reordering in FrameHeader.
#[test]
fn test_regression_detect_field_reordering() {
    let h1 = FrameHeader {
        frame_type: FrameType::Hello,
        request_id: RequestId::new(100),
        session_id: SessionId::new(200),
        tx_id: None,
        payload_length: 42,
        flags: 0,
        header_crc: 0x12345678,
    };

    let h2 = FrameHeader {
        frame_type: FrameType::RpcBatch,
        request_id: RequestId::new(101),
        session_id: SessionId::new(201),
        tx_id: Some(TransactionId::new(500)),
        payload_length: 1024,
        flags: 0,
        header_crc: 0x87654321,
    };

    assert_eq!(
        mem::size_of_val(&h1),
        mem::size_of_val(&h2),
        "Frame header size varies between instances"
    );
    assert!(validate_frame_header_layout().is_ok());
    assert_eq!(h1.frame_type, FrameType::Hello);
    assert_eq!(h2.frame_type, FrameType::RpcBatch);
    assert_eq!(h1.header_crc, 0x12345678);
    assert_eq!(h2.header_crc, 0x87654321);
}

/// Comprehensive validation of all protocol invariants.
///
/// This test runs the complete invariant suite to ensure protocol stability.
#[test]
fn test_master_protocol_invariant_validation() {
    assert!(
        ProtocolInvariants::validate_all().is_ok(),
        "Master protocol invariant validation failed"
    );

    let report = ProtocolInvariants::diagnostic_report();
    assert!(!report.is_empty(), "Diagnostic report is empty");
    assert!(
        report.contains("Protocol Invariants"),
        "Report missing header"
    );
    assert!(
        report.contains("Frame Header Layout"),
        "Report missing frame header section"
    );
    assert!(
        report.contains("Frame Type Codes"),
        "Report missing frame type section"
    );
    assert!(
        report.contains("Payload Kind Discriminators"),
        "Report missing payload kind section"
    );
    assert!(
        report.contains("Protocol Version Lock"),
        "Report missing protocol version section"
    );
}
