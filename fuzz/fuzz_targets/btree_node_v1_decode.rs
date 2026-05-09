#![no_main]

use andromeda_storage_page::BTreeNodeV1;
use libfuzzer_sys::fuzz_target;

mod common;

const MAX_BTREE_NODE_FUZZ_BYTES: usize = u16::MAX as usize;

fuzz_target!(|data: &[u8]| {
    let data = common::bounded_input(data, MAX_BTREE_NODE_FUZZ_BYTES);

    let Ok(decoded) = BTreeNodeV1::decode(data) else {
        return;
    };

    let decoded_key_count = decoded.key_count();
    let decoded_encoded_len = decoded.encoded_len();
    let Ok(decoded_for_page) = BTreeNodeV1::decode_for_page_id(data, decoded.header.page_id) else {
        return;
    };
    assert_eq!(decoded_for_page, decoded);

    if let Ok(encoded) = decoded.encode() {
        if let Ok(redecoded) = BTreeNodeV1::decode(&encoded) {
            // encode() returns a full page image; encoded_len() tracks the used prefix.
            assert!(decoded_encoded_len <= encoded.len());
            assert_eq!(decoded_encoded_len, decoded.header.free_start as usize);
            assert_eq!(encoded.len(), decoded.page_size() as usize);

            assert_eq!(redecoded, decoded);
            assert_eq!(redecoded.key_count(), decoded_key_count);
            assert_eq!(redecoded.encoded_len(), decoded_encoded_len);

            if let Ok(reencoded) = redecoded.encode() {
                assert_eq!(reencoded, encoded);
                assert_eq!(
                    redecoded.encoded_len(),
                    reencoded[..decoded_encoded_len].len()
                );
            }
        }
    }
});
