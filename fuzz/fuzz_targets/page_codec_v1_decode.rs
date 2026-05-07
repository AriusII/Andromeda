#![no_main]

use andromeda_storage::PageCodecV1;
use libfuzzer_sys::fuzz_target;

mod common;

fuzz_target!(|data: &[u8]| {
    let data = common::bounded_input(data, common::MAX_64K_INPUT_BYTES);
    common::ignore_decode(data, PageCodecV1::decode_header);
    common::ignore_decode(data, PageCodecV1::decode_trailer);

    let Ok(decoded) = PageCodecV1::decode_page(data) else {
        return;
    };

    let Ok(encoded) = PageCodecV1::encode_page(&decoded.header, &decoded.payload, &decoded.trailer)
    else {
        return;
    };
    assert_eq!(encoded, data);

    let Ok(redecoded) = PageCodecV1::decode_page(&encoded) else {
        return;
    };
    assert_eq!(redecoded, decoded);
});
