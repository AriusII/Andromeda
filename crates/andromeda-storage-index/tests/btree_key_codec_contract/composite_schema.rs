use andromeda_storage_index::{Key, KeyCodec};
use andromeda_storage_index::{KeyDatum, KeyScalarType};

#[test]
fn dec032_unsupported_datum_variants_are_rejected_for_composite_index_keys() {
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
        let result = KeyCodec::encode_composite_key(&[datum]);
        assert!(
            result.is_err(),
            "unsupported datum must not debug-string fallback into durable key"
        );
    }
}

#[test]
fn dec032_decode_composite_validates_schema_arity_and_fixed_types() {
    let encoded = KeyCodec::encode_composite_key(&[
        KeyDatum::Int32(10),
        KeyDatum::Int64(20),
        KeyDatum::Bool(false),
    ])
    .expect("encode");

    let decoded = KeyCodec::decode_composite(
        &encoded,
        &[
            KeyScalarType::Int32,
            KeyScalarType::Int64,
            KeyScalarType::Bool,
        ],
    )
    .expect("decode with matching schema");
    assert_eq!(
        decoded,
        vec![
            KeyDatum::Int32(10),
            KeyDatum::Int64(20),
            KeyDatum::Bool(false)
        ]
    );

    assert!(
        KeyCodec::decode_composite(&encoded, &[KeyScalarType::Int32, KeyScalarType::Int64])
            .is_err(),
        "schema arity mismatch must reject"
    );
    assert!(
        KeyCodec::decode_composite(
            &encoded,
            &[
                KeyScalarType::Int64,
                KeyScalarType::Int64,
                KeyScalarType::Bool
            ],
        )
        .is_err(),
        "schema type mismatch must reject"
    );
}

#[test]
fn dec032_decode_composite_rejects_variable_width_without_schema_type_identity() {
    let encoded =
        KeyCodec::encode_composite_key(&[KeyDatum::Text("a".to_string())]).expect("encode");
    assert!(
        KeyCodec::decode_composite(&encoded, &[KeyScalarType::Int32]).is_err(),
        "Text/Bytes composite columns need explicit schema type identity before durable decode"
    );
}

#[test]
fn test_codec_composite_mixed_roundtrip() {
    let datums = vec![
        KeyDatum::Int32(100),
        KeyDatum::Text("hello".to_string()),
        KeyDatum::Int64(999999),
    ];
    let key = Key::Composite(datums);
    let encoded = KeyCodec::encode_key(&key).expect("encode composite");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode composite");

    match decoded {
        Key::Composite(decoded_datums) => {
            assert_eq!(decoded_datums.len(), 3, "composite length mismatch");
        },
        _ => panic!("expected composite key, got {:?}", decoded),
    }
}

#[test]
fn test_codec_encode_composite_key() {
    let cols = vec![KeyDatum::Int32(42), KeyDatum::Text("test".to_string())];
    let encoded = KeyCodec::encode_composite_key(&cols).expect("encode composite");
    assert!(!encoded.is_empty(), "composite key should not be empty");
}

#[test]
fn test_codec_decode_composite_datums() {
    let cols = vec![
        KeyDatum::Int64(123),
        KeyDatum::Int32(456),
        KeyDatum::Bool(true),
    ];
    let encoded = KeyCodec::encode_composite_key(&cols).expect("encode");
    let schema = vec![
        KeyScalarType::Int64,
        KeyScalarType::Int32,
        KeyScalarType::Bool,
    ];
    let decoded = KeyCodec::decode_composite(&encoded, &schema).expect("decode");
    assert_eq!(decoded.len(), 3, "decoded composite should have 3 columns");
}

#[test]
fn test_codec_composite_with_null() {
    let cols = vec![KeyDatum::Null, KeyDatum::Int64(11), KeyDatum::Int32(42)];
    let encoded = KeyCodec::encode_composite_key(&cols).expect("encode");
    let schema = vec![
        KeyScalarType::Int32,
        KeyScalarType::Int64,
        KeyScalarType::Int32,
    ];
    let decoded = KeyCodec::decode_composite(&encoded, &schema).expect("decode");
    assert_eq!(
        decoded.len(),
        3,
        "composite with null should have 3 columns"
    );
}

#[test]
fn test_codec_empty_composite_key_error() {
    let cols: Vec<KeyDatum> = vec![];
    let result = KeyCodec::encode_composite_key(&cols);
    assert!(result.is_err(), "empty composite key should produce error");
}

#[test]
fn test_stress_large_composite_key() {
    let mut cols = vec![];
    for i in 0..20 {
        if i % 3 == 0 {
            cols.push(KeyDatum::Int32(i));
        } else if i % 3 == 1 {
            cols.push(KeyDatum::Text(format!("col_{}", i)));
        } else {
            cols.push(KeyDatum::Bytes(vec![i as u8; 10]));
        }
    }

    let encoded = KeyCodec::encode_composite_key(&cols).expect("encode");
    assert!(!encoded.is_empty(), "large composite key should encode");

    let encoded2 = KeyCodec::encode_composite_key(&cols).expect("encode again");
    assert_eq!(encoded, encoded2, "large composite determinism failed");
}
