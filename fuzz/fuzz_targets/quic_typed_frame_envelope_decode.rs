#![no_main]

use andromeda_quic::{
    FrameCodec, ResultStreamMetadataPolicy, TypedResultStreamBounds, TypedResultStreamContext,
    decode_typed_frame_envelope, validate_typed_result_stream_sequence_with_context_and_bounds,
    validate_typed_result_stream_sequence_with_metadata_policy,
};
use libfuzzer_sys::fuzz_target;

mod common;

const MAX_TYPED_SEQUENCE_FRAMES: usize = 128;

fuzz_target!(|data: &[u8]| {
    let data = common::bounded_input(data, common::MAX_64K_INPUT_BYTES);

    if let Ok((frame, consumed)) = FrameCodec::scan_one(data) {
        assert!(consumed <= data.len());
        let _ = frame.validate(frame.header.frame_type.stream_role());
        if let Ok(envelope) = decode_typed_frame_envelope(&frame) {
            let context = TypedResultStreamContext::from_envelope(&envelope);
            let _ = context.validate_frame_envelope(&frame);
        }
    }

    if let Ok(frames) = FrameCodec::scan_all(data) {
        if frames.is_empty() || frames.len() > MAX_TYPED_SEQUENCE_FRAMES {
            return;
        }

        let _ = validate_typed_result_stream_sequence_with_metadata_policy(
            &frames,
            ResultStreamMetadataPolicy::RowBatchRequired,
        );

        if let Ok(envelope) = decode_typed_frame_envelope(&frames[0]) {
            let context = TypedResultStreamContext::from_envelope(&envelope);
            let bounds = TypedResultStreamBounds::new(MAX_TYPED_SEQUENCE_FRAMES, data.len() as u64);
            let _ = validate_typed_result_stream_sequence_with_context_and_bounds(
                &frames,
                ResultStreamMetadataPolicy::RowBatchRequired,
                context,
                bounds,
            );
        }
    }
});
