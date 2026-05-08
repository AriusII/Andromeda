use andromeda_storage::btree_key_codec::{Key, KeyCodec};
use andromeda_storage::{Datum, ScalarType};

#[test]
fn dec032_unsupported_datum_variants_are_rejected_for_composite_index_keys() {
    for datum in [
        Datum::Int8(1),
        Datum::Int16(1),
        Datum::UInt8(1),
        Datum::UInt16(1),
        Datum::UInt32(1),
        Datum::UInt64(1),
        Datum::Float32(1.0),
        Datum::Float64(1.0),
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
    let encoded =
        KeyCodec::encode_composite_key(&[Datum::Int32(10), Datum::Int64(20), Datum::Bool(false)])
            .expect("encode");

    let decoded = KeyCodec::decode_composite(
        &encoded,
        &[ScalarType::Int32, ScalarType::Int64, ScalarType::Bool],
    )
    .expect("decode with matching schema");
    assert_eq!(
        decoded,
        vec![Datum::Int32(10), Datum::Int64(20), Datum::Bool(false)]
    );

    assert!(
        KeyCodec::decode_composite(&encoded, &[ScalarType::Int32, ScalarType::Int64]).is_err(),
        "schema arity mismatch must reject"
    );
    assert!(
        KeyCodec::decode_composite(
            &encoded,
            &[ScalarType::Int64, ScalarType::Int64, ScalarType::Bool],
        )
        .is_err(),
        "schema type mismatch must reject"
    );
}

#[test]
fn dec032_decode_composite_rejects_variable_width_without_schema_type_identity() {
    let encoded = KeyCodec::encode_composite_key(&[Datum::Text("a".to_string())]).expect("encode");
    assert!(
        KeyCodec::decode_composite(&encoded, &[ScalarType::Int32]).is_err(),
        "Text/Bytes composite columns need explicit schema type identity before durable decode"
    );
}

#[test]
fn test_codec_composite_mixed_roundtrip() {
    let datums = vec![
        Datum::Int32(100),
        Datum::Text("hello".to_string()),
        Datum::Int64(999999),
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
    let cols = vec![Datum::Int32(42), Datum::Text("test".to_string())];
    let encoded = KeyCodec::encode_composite_key(&cols).expect("encode composite");
    assert!(!encoded.is_empty(), "composite key should not be empty");
}

#[test]
fn test_codec_decode_composite_datums() {
    let cols = vec![Datum::Int64(123), Datum::Int32(456), Datum::Bool(true)];
    let encoded = KeyCodec::encode_composite_key(&cols).expect("encode");
    let schema = vec![ScalarType::Int64, ScalarType::Int32, ScalarType::Bool];
    let decoded = KeyCodec::decode_composite(&encoded, &schema).expect("decode");
    assert_eq!(decoded.len(), 3, "decoded composite should have 3 columns");
}

#[test]
fn test_codec_composite_with_null() {
    let cols = vec![Datum::Null, Datum::Int64(11), Datum::Int32(42)];
    let encoded = KeyCodec::encode_composite_key(&cols).expect("encode");
    let schema = vec![ScalarType::Int32, ScalarType::Int64, ScalarType::Int32];
    let decoded = KeyCodec::decode_composite(&encoded, &schema).expect("decode");
    assert_eq!(
        decoded.len(),
        3,
        "composite with null should have 3 columns"
    );
}

#[test]
fn test_codec_empty_composite_key_error() {
    let cols: Vec<Datum> = vec![];
    let result = KeyCodec::encode_composite_key(&cols);
    assert!(result.is_err(), "empty composite key should produce error");
}

#[test]
fn test_stress_large_composite_key() {
    let mut cols = vec![];
    for i in 0..20 {
        if i % 3 == 0 {
            cols.push(Datum::Int32(i));
        } else if i % 3 == 1 {
            cols.push(Datum::Text(format!("col_{}", i)));
        } else {
            cols.push(Datum::Bytes(vec![i as u8; 10]));
        }
    }

    let encoded = KeyCodec::encode_composite_key(&cols).expect("encode");
    assert!(!encoded.is_empty(), "large composite key should encode");

    let encoded2 = KeyCodec::encode_composite_key(&cols).expect("encode again");
    assert_eq!(encoded, encoded2, "large composite determinism failed");
}
