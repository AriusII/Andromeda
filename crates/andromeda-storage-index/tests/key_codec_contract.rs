use andromeda_storage_index::{Key, KeyCodec, KeyComparator, KeyDatum, KeyScalarType};
use std::cmp::Ordering;

#[test]
fn key_codec_preserves_text_ordering() {
    let left = KeyCodec::encode_key(&Key::Text("a".to_string())).expect("encode left");
    let right = KeyCodec::encode_key(&Key::Text("a\0b".to_string())).expect("encode right");

    assert_eq!(KeyComparator::compare(&left, &right), Ordering::Less);
}

#[test]
fn key_codec_rejects_unsupported_composite_datum_variants() {
    for datum in [
        KeyDatum::Int8(1),
        KeyDatum::Int16(1),
        KeyDatum::UInt8(1),
        KeyDatum::UInt16(1),
        KeyDatum::UInt32(1),
        KeyDatum::UInt64(1),
        KeyDatum::Float32(1.0),
        KeyDatum::Float64(1.0),
    ] {
        assert!(KeyCodec::encode_composite_key(&[datum]).is_err());
    }
}

#[test]
fn key_codec_validates_composite_schema() {
    let encoded = KeyCodec::encode_composite_key(&[
        KeyDatum::Int32(10),
        KeyDatum::Int64(20),
        KeyDatum::Bool(false),
    ])
    .expect("encode composite");

    let decoded = KeyCodec::decode_composite(
        &encoded,
        &[
            KeyScalarType::Int32,
            KeyScalarType::Int64,
            KeyScalarType::Bool,
        ],
    )
    .expect("decode composite");
    assert_eq!(
        decoded,
        vec![
            KeyDatum::Int32(10),
            KeyDatum::Int64(20),
            KeyDatum::Bool(false),
        ]
    );
}
