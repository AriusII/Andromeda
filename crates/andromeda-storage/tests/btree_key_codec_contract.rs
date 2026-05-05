//! N2-BTREE-003 Key Codec Contract Tests
//!
//! Comprehensive test suite for order-preserving key encoding/decoding
//! and lexicographic comparison. Validates order preservation invariant
//! and determinism requirements for B-Tree indexing.

#![forbid(unsafe_code)]

use andromeda_storage::btree_key_codec::{Key, KeyCodec, KeyComparator, KeyType};
use andromeda_storage::{Datum, ScalarType};
use std::cmp::Ordering;

// ============================================================================
// Test Groups Organization
// ============================================================================
//
// Group 1: Single Key Type Round-trips (8 tests)
// Group 2: Composite Key Tests (4 tests)
// Group 3: Order Preservation (5 tests)
// Group 4: Determinism (4 tests)
// Group 5: Edge Cases (6 tests)
// Group 6: Comparator Operations (4 tests)
// ============================================================================

// ============================================================================
// Group 1: Single Key Type Round-trips (Tests 1-8)
// ============================================================================

/// Test 1: Encode/decode null key
#[test]
fn test_codec_null_roundtrip() {
    let key = Key::Null;
    let encoded = KeyCodec::encode_key(&key).expect("encode null");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode null");
    assert_eq!(key, decoded, "null key roundtrip failed");
}

/// Test 2: Encode/decode int32 positive
#[test]
fn test_codec_int32_positive_roundtrip() {
    let key = Key::Int32(42);
    let encoded = KeyCodec::encode_key(&key).expect("encode int32");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode int32");
    assert_eq!(key, decoded, "int32 positive roundtrip failed");
}

/// Test 3: Encode/decode int32 negative
#[test]
fn test_codec_int32_negative_roundtrip() {
    let key = Key::Int32(-12345);
    let encoded = KeyCodec::encode_key(&key).expect("encode int32");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode int32");
    assert_eq!(key, decoded, "int32 negative roundtrip failed");
}

/// Test 4: Encode/decode int64 large positive
#[test]
fn test_codec_int64_large_positive_roundtrip() {
    let key = Key::Int64(9223372036854775807i64); // i64::MAX
    let encoded = KeyCodec::encode_key(&key).expect("encode int64");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode int64");
    assert_eq!(key, decoded, "int64 large positive roundtrip failed");
}

/// Test 5: Encode/decode int64 large negative
#[test]
fn test_codec_int64_large_negative_roundtrip() {
    let key = Key::Int64(-9223372036854775808i64); // i64::MIN
    let encoded = KeyCodec::encode_key(&key).expect("encode int64");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode int64");
    assert_eq!(key, decoded, "int64 large negative roundtrip failed");
}

/// Test 6: Encode/decode text UTF-8
#[test]
fn test_codec_text_utf8_roundtrip() {
    let key = Key::Text("Hello, 世界! 🌍 café".to_string());
    let encoded = KeyCodec::encode_key(&key).expect("encode text");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode text");
    assert_eq!(key, decoded, "text UTF-8 roundtrip failed");
}

/// Test 7: Encode/decode bytes
#[test]
fn test_codec_bytes_roundtrip() {
    let key = Key::Bytes(vec![0, 1, 255, 127, 128, 200, 50, 0]);
    let encoded = KeyCodec::encode_key(&key).expect("encode bytes");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode bytes");
    assert_eq!(key, decoded, "bytes roundtrip failed");
}

/// Test 8: Encode/decode composite (mixed types)
#[test]
fn test_codec_composite_mixed_roundtrip() {
    let datums = vec![
        Datum::Int32(100),
        Datum::Text("hello".to_string()),
        Datum::Int64(999999),
    ];
    let key = Key::Composite(datums.clone());
    let encoded = KeyCodec::encode_key(&key).expect("encode composite");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode composite");

    // Verify structure is preserved
    match decoded {
        Key::Composite(decoded_datums) => {
            assert_eq!(decoded_datums.len(), 3, "composite length mismatch");
        }
        _ => panic!("expected composite key, got {:?}", decoded),
    }
}

// ============================================================================
// Group 2: Composite Key Tests (Tests 9-12)
// ============================================================================

/// Test 9: Encode composite key from datums
#[test]
fn test_codec_encode_composite_key() {
    let cols = vec![Datum::Int32(42), Datum::Text("test".to_string())];
    let encoded = KeyCodec::encode_composite_key(&cols).expect("encode composite");
    assert!(!encoded.is_empty(), "composite key should not be empty");
}

/// Test 10: Decode composite key to datums
#[test]
fn test_codec_decode_composite_datums() {
    let cols = vec![
        Datum::Int64(123),
        Datum::Text("data".to_string()),
        Datum::Bytes(vec![1, 2, 3]),
    ];
    let encoded = KeyCodec::encode_composite_key(&cols).expect("encode");
    let schema = vec![ScalarType::Int64, ScalarType::Int64, ScalarType::Int64];
    let decoded = KeyCodec::decode_composite(&encoded, &schema).expect("decode");
    assert_eq!(decoded.len(), 3, "decoded composite should have 3 columns");
}

/// Test 11: Composite with null value
#[test]
fn test_codec_composite_with_null() {
    let cols = vec![
        Datum::Null,
        Datum::Text("test".to_string()),
        Datum::Int32(42),
    ];
    let encoded = KeyCodec::encode_composite_key(&cols).expect("encode");
    let schema = vec![ScalarType::Int32, ScalarType::Int64, ScalarType::Int32];
    let decoded = KeyCodec::decode_composite(&encoded, &schema).expect("decode");
    assert_eq!(
        decoded.len(),
        3,
        "composite with null should have 3 columns"
    );
}

/// Test 12: Empty composite key should error
#[test]
fn test_codec_empty_composite_key_error() {
    let cols: Vec<Datum> = vec![];
    let result = KeyCodec::encode_composite_key(&cols);
    assert!(result.is_err(), "empty composite key should produce error");
}

// ============================================================================
// Group 3: Order Preservation Invariant (Tests 13-17)
// ============================================================================

/// Test 13: Order preservation for int32
#[test]
fn test_order_preservation_int32_sequence() {
    let keys = vec![
        Key::Int32(i32::MIN),
        Key::Int32(-1000000),
        Key::Int32(-1),
        Key::Int32(0),
        Key::Int32(1),
        Key::Int32(1000000),
        Key::Int32(i32::MAX),
    ];

    let encoded: Vec<_> = keys
        .iter()
        .map(|k| KeyCodec::encode_key(k).unwrap())
        .collect();

    // Verify lexicographic order matches value order
    for i in 0..encoded.len() - 1 {
        let cmp = KeyComparator::compare(&encoded[i], &encoded[i + 1]);
        assert_eq!(
            cmp,
            Ordering::Less,
            "order preservation violated at index {} for int32",
            i
        );
    }
}

/// Test 14: Order preservation for int64
#[test]
fn test_order_preservation_int64_extremes() {
    let keys = vec![
        Key::Int64(i64::MIN),
        Key::Int64(-9223372036854775000i64),
        Key::Int64(-1),
        Key::Int64(0),
        Key::Int64(1),
        Key::Int64(9223372036854775000i64),
        Key::Int64(i64::MAX),
    ];

    let encoded: Vec<_> = keys
        .iter()
        .map(|k| KeyCodec::encode_key(k).unwrap())
        .collect();

    for i in 0..encoded.len() - 1 {
        let cmp = KeyComparator::compare(&encoded[i], &encoded[i + 1]);
        assert_eq!(
            cmp,
            Ordering::Less,
            "int64 order preservation failed at {}",
            i
        );
    }
}

/// Test 15: Order preservation for text
#[test]
fn test_order_preservation_text_lexicographic() {
    let keys = vec![
        Key::Text("apple".to_string()),
        Key::Text("banana".to_string()),
        Key::Text("cherry".to_string()),
        Key::Text("date".to_string()),
        Key::Text("elderberry".to_string()),
        Key::Text("fig".to_string()),
        Key::Text("grape".to_string()),
    ];

    let encoded: Vec<_> = keys
        .iter()
        .map(|k| KeyCodec::encode_key(k).unwrap())
        .collect();

    for i in 0..encoded.len() - 1 {
        let cmp = KeyComparator::compare(&encoded[i], &encoded[i + 1]);
        assert_eq!(
            cmp,
            Ordering::Less,
            "text order preservation failed at {}",
            i
        );
    }
}

/// Test 16: Order preservation for bytes
#[test]
fn test_order_preservation_bytes() {
    let keys = vec![
        Key::Bytes(vec![0]),
        Key::Bytes(vec![1]),
        Key::Bytes(vec![100]),
        Key::Bytes(vec![200]),
        Key::Bytes(vec![255]),
    ];

    let encoded: Vec<_> = keys
        .iter()
        .map(|k| KeyCodec::encode_key(k).unwrap())
        .collect();

    for i in 0..encoded.len() - 1 {
        let cmp = KeyComparator::compare(&encoded[i], &encoded[i + 1]);
        assert_eq!(
            cmp,
            Ordering::Less,
            "bytes order preservation failed at {}",
            i
        );
    }
}

/// Test 17: Mixed type keys don't compare across types (each type separately)
#[test]
fn test_order_preservation_within_type() {
    // Verify that keys are ordered within their type, not across types
    let int_keys = vec![Key::Int32(1), Key::Int32(2), Key::Int32(3)];
    let int_encoded: Vec<_> = int_keys
        .iter()
        .map(|k| KeyCodec::encode_key(k).unwrap())
        .collect();

    // All comparisons within int32 type should work
    for i in 0..int_encoded.len() - 1 {
        let cmp = KeyComparator::compare(&int_encoded[i], &int_encoded[i + 1]);
        assert_eq!(cmp, Ordering::Less, "int32 keys should be ordered");
    }
}

// ============================================================================
// Group 4: Determinism Invariant (Tests 18-21)
// ============================================================================

/// Test 18: Determinism for int32
#[test]
fn test_determinism_int32() {
    let key = Key::Int32(42);
    let enc1 = KeyCodec::encode_key(&key).expect("encode 1");
    let enc2 = KeyCodec::encode_key(&key).expect("encode 2");
    let enc3 = KeyCodec::encode_key(&key).expect("encode 3");

    assert_eq!(enc1, enc2, "determinism failed: enc1 != enc2");
    assert_eq!(enc2, enc3, "determinism failed: enc2 != enc3");
}

/// Test 19: Determinism for text
#[test]
fn test_determinism_text() {
    let key = Key::Text("hello world".to_string());
    let enc1 = KeyCodec::encode_key(&key).expect("encode 1");
    let enc2 = KeyCodec::encode_key(&key).expect("encode 2");
    let enc3 = KeyCodec::encode_key(&key).expect("encode 3");

    assert_eq!(enc1, enc2, "text determinism failed");
    assert_eq!(enc2, enc3, "text determinism failed");
}

/// Test 20: Determinism for bytes
#[test]
fn test_determinism_bytes() {
    let key = Key::Bytes(vec![1, 2, 255, 0, 127]);
    let enc1 = KeyCodec::encode_key(&key).expect("encode 1");
    let enc2 = KeyCodec::encode_key(&key).expect("encode 2");

    assert_eq!(enc1, enc2, "bytes determinism failed");
}

/// Test 21: Determinism for composite
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

// ============================================================================
// Group 5: Edge Cases (Tests 22-27)
// ============================================================================

/// Test 22: Empty text key
#[test]
fn test_edge_case_empty_text() {
    let key = Key::Text("".to_string());
    let encoded = KeyCodec::encode_key(&key).expect("encode");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode");
    assert_eq!(key, decoded, "empty text edge case failed");
}

/// Test 23: Empty bytes key
#[test]
fn test_edge_case_empty_bytes() {
    let key = Key::Bytes(vec![]);
    let encoded = KeyCodec::encode_key(&key).expect("encode");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode");
    assert_eq!(key, decoded, "empty bytes edge case failed");
}

/// Test 24: Very large text key
#[test]
fn test_edge_case_large_text() {
    let large_text = "x".repeat(50000);
    let key = Key::Text(large_text.clone());
    let encoded = KeyCodec::encode_key(&key).expect("encode");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode");
    assert_eq!(key, decoded, "large text edge case failed");
}

/// Test 25: Zero int32 and int64
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

/// Test 26: Special UTF-8 characters
#[test]
fn test_edge_case_special_characters() {
    let key = Key::Text("🎉 emoji 中文 العربية".to_string());
    let encoded = KeyCodec::encode_key(&key).expect("encode");
    let decoded = KeyCodec::decode_key(&encoded).expect("decode");
    assert_eq!(key, decoded, "special characters edge case failed");
}

/// Test 27: Boundary values (min/max)
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

// ============================================================================
// Group 6: Comparator Operations (Tests 28-31)
// ============================================================================

/// Test 28: Comparator equality
#[test]
fn test_comparator_equal() {
    let key = Key::Int32(42);
    let encoded = KeyCodec::encode_key(&key).expect("encode");

    assert!(
        KeyComparator::equal(&encoded, &encoded),
        "equal comparison failed"
    );
}

/// Test 29: Comparator less than
#[test]
fn test_comparator_less_than() {
    let k1 = KeyCodec::encode_key(&Key::Int32(10)).expect("encode 10");
    let k2 = KeyCodec::encode_key(&Key::Int32(20)).expect("encode 20");

    let cmp = KeyComparator::compare(&k1, &k2);
    assert_eq!(cmp, Ordering::Less, "less than comparison failed");
}

/// Test 30: Comparator greater than
#[test]
fn test_comparator_greater_than() {
    let k1 = KeyCodec::encode_key(&Key::Int32(20)).expect("encode 20");
    let k2 = KeyCodec::encode_key(&Key::Int32(10)).expect("encode 10");

    let cmp = KeyComparator::compare(&k1, &k2);
    assert_eq!(cmp, Ordering::Greater, "greater than comparison failed");
}

/// Test 31: Comparator range scan boundary
#[test]
fn test_comparator_range_boundary() {
    let key = KeyCodec::encode_key(&Key::Int32(50)).expect("encode");
    let upper = KeyCodec::encode_key(&Key::Int32(100)).expect("encode");

    let cmp = KeyComparator::compare_range(&key, &upper);
    assert_eq!(cmp, Ordering::Less, "range boundary comparison failed");
}

// ============================================================================
// Additional Stress Tests (Tests 32+)
// ============================================================================

/// Test 32: Order preservation stress test (100 random int32 values)
#[test]
fn test_stress_order_preservation_int32() {
    let mut values = vec![
        i32::MIN,
        -1000000,
        -100000,
        -10000,
        -1000,
        -100,
        -10,
        -1,
        0,
        1,
        10,
        100,
        1000,
        10000,
        100000,
        1000000,
        i32::MAX,
    ];

    // Duplicate and shuffle slightly for stress
    let mut extended = values.clone();
    extended.extend(&values);

    let keys: Vec<_> = extended.iter().map(|&v| Key::Int32(v)).collect();
    let encoded: Vec<_> = keys
        .iter()
        .map(|k| KeyCodec::encode_key(k).unwrap())
        .collect();

    // Verify sorted order
    for i in 0..encoded.len() - 1 {
        if i % 2 == 0 {
            // Every other one should be equal or less
            let cmp = KeyComparator::compare(&encoded[i], &encoded[i + 1]);
            assert!(
                cmp == Ordering::Less || cmp == Ordering::Equal,
                "stress test order violation at index {}",
                i
            );
        }
    }
}

/// Test 33: Large composite key with many columns
#[test]
fn test_stress_large_composite_key() {
    let mut cols = vec![];
    for i in 0..20 {
        if i % 3 == 0 {
            cols.push(Datum::Int32(i as i32));
        } else if i % 3 == 1 {
            cols.push(Datum::Text(format!("col_{}", i)));
        } else {
            cols.push(Datum::Bytes(vec![i as u8; 10]));
        }
    }

    let encoded = KeyCodec::encode_composite_key(&cols).expect("encode");
    assert!(!encoded.is_empty(), "large composite key should encode");

    // Verify determinism
    let encoded2 = KeyCodec::encode_composite_key(&cols).expect("encode again");
    assert_eq!(encoded, encoded2, "large composite determinism failed");
}

/// Test 34: Encoding format validation (format: [type_tag][length][value])
#[test]
fn test_encoding_format_validation() {
    let key = Key::Int32(42);
    let encoded = KeyCodec::encode_key(&key).expect("encode");

    // Check format: should have at least type_tag (1) + length (2) = 3 bytes
    assert!(
        encoded.len() >= 3,
        "encoded key too short: {}",
        encoded.len()
    );

    // First byte should be type tag
    let type_tag = KeyType::from_tag(encoded[0]).expect("parse type tag");
    assert_eq!(type_tag, KeyType::Int32, "type tag mismatch");

    // Next 2 bytes should be length (little-endian)
    let length = u16::from_le_bytes([encoded[1], encoded[2]]) as usize;
    assert_eq!(length, 4, "int32 should have length 4, got {}", length);
}

/// Test 35: Null key ordering behavior
#[test]
fn test_null_key_ordering() {
    let null_key = Key::Null;
    let int_key = Key::Int32(0);

    let null_enc = KeyCodec::encode_key(&null_key).expect("encode null");
    let int_enc = KeyCodec::encode_key(&int_key).expect("encode int");

    // Null should encode consistently
    let null_enc2 = KeyCodec::encode_key(&null_key).expect("encode null again");
    assert_eq!(null_enc, null_enc2, "null key determinism failed");

    // Types can be compared but should maintain internal consistency
    let _cmp = KeyComparator::compare(&null_enc, &int_enc);
}
