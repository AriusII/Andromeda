use andromeda_storage::btree_key_codec::{Key, KeyCodec};

#[test]
fn test_codec_null_roundtrip() {
    let key = Key::Null;
    let encoded = KeyCodec::encode_key(&key).expect("encode null");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode null");
    assert_eq!(key, decoded, "null key roundtrip failed");
}

#[test]
fn test_codec_int32_positive_roundtrip() {
    let key = Key::Int32(42);
    let encoded = KeyCodec::encode_key(&key).expect("encode int32");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode int32");
    assert_eq!(key, decoded, "int32 positive roundtrip failed");
}

#[test]
fn test_codec_int32_negative_roundtrip() {
    let key = Key::Int32(-12345);
    let encoded = KeyCodec::encode_key(&key).expect("encode int32");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode int32");
    assert_eq!(key, decoded, "int32 negative roundtrip failed");
}

#[test]
fn test_codec_int64_large_positive_roundtrip() {
    let key = Key::Int64(9223372036854775807i64);
    let encoded = KeyCodec::encode_key(&key).expect("encode int64");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode int64");
    assert_eq!(key, decoded, "int64 large positive roundtrip failed");
}

#[test]
fn test_codec_int64_large_negative_roundtrip() {
    let key = Key::Int64(-9223372036854775808i64);
    let encoded = KeyCodec::encode_key(&key).expect("encode int64");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode int64");
    assert_eq!(key, decoded, "int64 large negative roundtrip failed");
}

#[test]
fn test_codec_text_utf8_roundtrip() {
    let key = Key::Text("Hello, 世界! 🌍 café".to_string());
    let encoded = KeyCodec::encode_key(&key).expect("encode text");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode text");
    assert_eq!(key, decoded, "text UTF-8 roundtrip failed");
}

#[test]
fn test_codec_bytes_roundtrip() {
    let key = Key::Bytes(vec![0, 1, 255, 127, 128, 200, 50, 0]);
    let encoded = KeyCodec::encode_key(&key).expect("encode bytes");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode bytes");
    assert_eq!(key, decoded, "bytes roundtrip failed");
}

#[test]
fn test_edge_case_empty_text() {
    let key = Key::Text("".to_string());
    let encoded = KeyCodec::encode_key(&key).expect("encode");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode");
    assert_eq!(key, decoded, "empty text edge case failed");
}

#[test]
fn test_edge_case_empty_bytes() {
    let key = Key::Bytes(vec![]);
    let encoded = KeyCodec::encode_key(&key).expect("encode");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode");
    assert_eq!(key, decoded, "empty bytes edge case failed");
}

#[test]
fn test_edge_case_large_text() {
    let large_text = "x".repeat(50000);
    let key = Key::Text(large_text.clone());
    let encoded = KeyCodec::encode_key(&key).expect("encode");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode");
    assert_eq!(key, decoded, "large text edge case failed");
}

#[test]
fn test_edge_case_zero() {
    let key32 = Key::Int32(0);
    let key64 = Key::Int64(0);

    let enc32 = KeyCodec::encode_key(&key32).expect("encode int32");
    let dec32 = KeyCodec::decode_key(&enc32).expect("decode int32");
    assert_eq!(key32, dec32, "zero int32 failed");

    let enc64 = KeyCodec::encode_key(&key64).expect("encode int64");
    let dec64 = KeyCodec::decode_key(&enc64).expect("decode int64");
    assert_eq!(key64, dec64, "zero int64 failed");
}

#[test]
fn test_edge_case_special_characters() {
    let key = Key::Text("🎉 emoji 中文 العربية".to_string());
    let encoded = KeyCodec::encode_key(&key).expect("encode");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode");
    assert_eq!(key, decoded, "special characters edge case failed");
}

#[test]
fn test_edge_case_min_max_values() {
    let keys = vec![
        Key::Int32(i32::MIN),
        Key::Int32(i32::MAX),
        Key::Int64(i64::MIN),
        Key::Int64(i64::MAX),
    ];

    for key in keys {
        let encoded = KeyCodec::encode_key(&key).expect("encode");
        let decoded = KeyCodec::decode_key(&encoded).expect("decode");
        assert_eq!(key, decoded, "min/max value failed for {:?}", key);
    }
}
