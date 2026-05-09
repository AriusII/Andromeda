#![no_main]

use andromeda_rpc_protocol::frame::{FrameCodec, validate_single_frame_on_stream};
use libfuzzer_sys::fuzz_target;

mod common;

fuzz_target!(|data: &[u8]| {
    let data = common::bounded_input(data, common::MAX_64K_INPUT_BYTES);

    if let Ok((frame, consumed)) = FrameCodec::scan_one(data) {
        assert!(consumed <= data.len());
        let stream_role = frame.header.frame_type.stream_role();
        let _ = validate_single_frame_on_stream(&frame, stream_role);
        if let Ok(encoded) = FrameCodec::encode(&frame) {
            let _ = FrameCodec::decode(&encoded);
        }
    }

    common::ignore_decode(data, FrameCodec::decode);
    common::ignore_decode(data, FrameCodec::scan_all);
});
