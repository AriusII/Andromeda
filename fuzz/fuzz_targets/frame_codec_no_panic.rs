#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = data.len();
    let _ = andromeda_quic::validate_frame_header_layout();
});
