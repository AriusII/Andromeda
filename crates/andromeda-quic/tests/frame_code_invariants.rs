use andromeda_quic::{
    AUTH_FRAME_CODE, HELLO_FRAME_CODE, MAX_FRAME_PAYLOAD_LENGTH, RPC_BATCH_FRAME_CODE,
    RPC_EXECUTE_REQUEST_FRAME_CODE, TELEMETRY_SOFT_SIGNAL_FRAME_CODE,
};

#[test]
fn frame_type_codes_are_locked() {
    assert_eq!(HELLO_FRAME_CODE, 1);
    assert_eq!(AUTH_FRAME_CODE, 2);
    assert_eq!(RPC_EXECUTE_REQUEST_FRAME_CODE, 5);
    assert_eq!(RPC_BATCH_FRAME_CODE, 7);
    assert_eq!(TELEMETRY_SOFT_SIGNAL_FRAME_CODE, 100);
}

#[test]
fn max_frame_payload_is_16_mib() {
    assert_eq!(MAX_FRAME_PAYLOAD_LENGTH, 16 * 1024 * 1024);
}
