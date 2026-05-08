use super::{Key, KeyCodec};

#[allow(dead_code)]
pub(super) fn assert_golden_roundtrip(key: Key, expected: &[u8]) {
    let encoded = KeyCodec::encode_key(&key).expect("encode golden key");
    assert_eq!(encoded, expected, "golden encoding drift for {:?}", key);

    let decoded = KeyCodec::decode_key(expected).expect("decode golden key");
    assert_eq!(decoded, key, "golden decoding drift");
}
