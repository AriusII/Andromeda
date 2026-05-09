use super::*;

/// Validates that FrameHeader wire offsets remain constant.
///
/// The encoded frame header is wire-critical. Rust struct padding may differ
/// from the wire length and must not be treated as the protocol contract.
#[test]
fn test_frame_header_offset_stability() {
    assert!(
        size_of::<FrameHeader>() >= FRAME_CODEC_HEADER_LEN,
        "FrameHeader memory layout must be large enough for the encoded header"
    );

    assert!(
        validate_frame_header_layout().is_ok(),
        "Frame header layout validation failed"
    );

    assert_eq!(
        FRAME_CODEC_HEADER_LEN, 52,
        "FRAME_CODEC_HEADER_LEN changed! This breaks wire protocol compatibility."
    );

    let expected_total = 4 + 8 + 8 + 16 + 8 + 4 + 4;
    assert_eq!(
        expected_total, 52,
        "Field size calculation incorrect: {}",
        expected_total
    );
}
