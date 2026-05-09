#![no_main]

use andromeda_rpc_protocol::{FrameCodec, validate_frame_header_layout};
use libfuzzer_sys::fuzz_target;

mod common;

fuzz_target!(|data: &[u8]| {
    let data = common::bounded_input(data, common::MAX_64K_INPUT_BYTES);
    let _ = validate_frame_header_layout();
    common::ignore_decode(data, FrameCodec::decode);
    common::ignore_decode(data, FrameCodec::scan_all);
});
