use andromeda_storage_index::{Key, KeyCodec};

pub(crate) fn assert_golden(key: Key, expected: &[u8]) {
    let encoded = KeyCodec::encode_key(&key).expect("encode golden key");
    assert_eq!(encoded, expected, "golden encoding drift for {:?}", key);

    let decoded = KeyCodec::decode_key(expected).expect("decode golden key");
    assert_eq!(decoded, key, "golden decoding drift");
}
