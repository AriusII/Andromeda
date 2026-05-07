#![no_main]

use andromeda_storage::PageCodecV1;
use libfuzzer_sys::fuzz_target;

const MAX_PAGE_CODEC_FUZZ_BYTES: usize = 64 * 1024;

fuzz_target!(|data: &[u8]| {
    let data = &data[..data.len().min(MAX_PAGE_CODEC_FUZZ_BYTES)];
    let _ = PageCodecV1::decode_header(data);
    let _ = PageCodecV1::decode_trailer(data);

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
