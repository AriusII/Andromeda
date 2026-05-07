use andromeda_storage::Datum;
use andromeda_storage::btree_key_codec::{Key, KeyCodec};

#[test]
fn test_determinism_int32() {
    let key = Key::Int32(42);
    let enc1 = KeyCodec::encode_key(&key).expect("encode 1");
    let enc2 = KeyCodec::encode_key(&key).expect("encode 2");
    let enc3 = KeyCodec::encode_key(&key).expect("encode 3");

    assert_eq!(enc1, enc2, "determinism failed: enc1 != enc2");
    assert_eq!(enc2, enc3, "determinism failed: enc2 != enc3");
}

#[test]
fn test_determinism_text() {
    let key = Key::Text("hello world".to_string());
    let enc1 = KeyCodec::encode_key(&key).expect("encode 1");
    let enc2 = KeyCodec::encode_key(&key).expect("encode 2");
    let enc3 = KeyCodec::encode_key(&key).expect("encode 3");

    assert_eq!(enc1, enc2, "text determinism failed");
    assert_eq!(enc2, enc3, "text determinism failed");
}

#[test]
fn test_determinism_bytes() {
    let key = Key::Bytes(vec![1, 2, 255, 0, 127]);
    let enc1 = KeyCodec::encode_key(&key).expect("encode 1");
    let enc2 = KeyCodec::encode_key(&key).expect("encode 2");

    assert_eq!(enc1, enc2, "bytes determinism failed");
}

#[test]
fn test_determinism_composite() {
    let cols = vec![
        Datum::Int32(42),
        Datum::Text("data".to_string()),
        Datum::Bytes(vec![1, 2, 3]),
    ];
    let enc1 = KeyCodec::encode_composite_key(&cols).expect("encode 1");
    let enc2 = KeyCodec::encode_composite_key(&cols).expect("encode 2");

    assert_eq!(enc1, enc2, "composite determinism failed");
}
