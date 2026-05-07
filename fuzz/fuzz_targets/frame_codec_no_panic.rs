#![no_main]

use andromeda_quic::FrameCodec;
use libfuzzer_sys::fuzz_target;

const MAX_FRAME_FUZZ_BYTES: usize = 64 * 1024;

fuzz_target!(|data: &[u8]| {
    let data = &data[..data.len().min(MAX_FRAME_FUZZ_BYTES)];
    let _ = andromeda_quic::validate_frame_header_layout();
    let _ = FrameCodec::decode(data);
    let _ = FrameCodec::scan_all(data);
});
