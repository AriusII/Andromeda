#![no_main]

use andromeda_quic::FrameCodec;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = andromeda_quic::validate_frame_header_layout();
    let _ = FrameCodec::decode(data);
    let _ = FrameCodec::scan_all(data);
});
