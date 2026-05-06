#![no_main]

use andromeda_storage::PageCodecV1;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = PageCodecV1::decode_header(data);
    let _ = PageCodecV1::decode_trailer(data);

    let Ok(decoded) = PageCodecV1::decode_page(data) else {
        return;
    };

    let encoded = PageCodecV1::encode_page(&decoded.header, &decoded.payload, &decoded.trailer)
        .expect("decoded page should re-encode");
    assert_eq!(encoded, data);

    let redecoded = PageCodecV1::decode_page(&encoded).expect("encoded page should decode");
    assert_eq!(redecoded, decoded);
});
