use andromeda_storage::btree_key_codec::{Key, KeyCodec, KeyComparator};
use std::cmp::Ordering;

#[test]
fn test_comparator_equal() {
    let key = Key::Int32(42);
    let encoded = KeyCodec::encode_key(&key).expect("encode");

    assert!(
        KeyComparator::equal(&encoded, &encoded),
        "equal comparison failed"
    );
}

#[test]
fn test_comparator_less_than() {
    let k1 = KeyCodec::encode_key(&Key::Int32(10)).expect("encode 10");
    let k2 = KeyCodec::encode_key(&Key::Int32(20)).expect("encode 20");

    let cmp = KeyComparator::compare(&k1, &k2);
    assert_eq!(cmp, Ordering::Less, "less than comparison failed");
}

#[test]
fn test_comparator_greater_than() {
    let k1 = KeyCodec::encode_key(&Key::Int32(20)).expect("encode 20");
    let k2 = KeyCodec::encode_key(&Key::Int32(10)).expect("encode 10");

    let cmp = KeyComparator::compare(&k1, &k2);
    assert_eq!(cmp, Ordering::Greater, "greater than comparison failed");
}

#[test]
fn test_comparator_range_boundary() {
    let key = KeyCodec::encode_key(&Key::Int32(50)).expect("encode");
    let upper = KeyCodec::encode_key(&Key::Int32(100)).expect("encode");

    let cmp = KeyComparator::compare_range(&key, &upper);
    assert_eq!(cmp, Ordering::Less, "range boundary comparison failed");
}

#[test]
fn test_encoding_format_validation() {
    let key = Key::Int32(42);
    let encoded = KeyCodec::encode_key(&key).expect("encode");

    assert!(
        encoded.len() >= 3,
        "encoded key too short: {}",
        encoded.len()
    );

    assert_eq!(encoded[0], 1, "int32 type tag mismatch");

    let length = u16::from_le_bytes([encoded[1], encoded[2]]) as usize;
    assert_eq!(length, 4, "int32 should have length 4, got {}", length);
}
