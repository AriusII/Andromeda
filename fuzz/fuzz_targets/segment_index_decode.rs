#![no_main]

use andromeda_segment::segment_index::SegmentIndexV0;
use libfuzzer_sys::fuzz_target;

mod common;

fuzz_target!(|data: &[u8]| {
    let data = common::bounded_input(data, common::MAX_4K_PAYLOAD_BYTES);
    let Ok(decoded) = SegmentIndexV0::decode(data) else {
        return;
    };

    assert!(decoded.validate().is_ok());

    let encoded = decoded.encode();
    assert_eq!(encoded.as_deref().ok(), Some(data));

    if let Ok(encoded) = encoded {
        let redecoded = SegmentIndexV0::decode(&encoded);
        assert_eq!(redecoded.as_ref().ok(), Some(&decoded));
    }
});
