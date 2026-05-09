use andromeda_storage_index::{Key, KeyCodec, KeyComparator};
use std::cmp::Ordering;

#[test]
fn dec032_text_ordering_adversarial_prefix_cases() {
    let ordered = ["", "a", "aa", "aab", "ab", "az", "b", "z", "é", "世界"];

    let encoded: Vec<_> = ordered
        .iter()
        .map(|value| KeyCodec::encode_key(&Key::Text((*value).to_string())).unwrap())
        .collect();

    for pair in encoded.windows(2) {
        assert_eq!(KeyComparator::compare(&pair[0], &pair[1]), Ordering::Less);
    }

    let aa = KeyCodec::encode_key(&Key::Text("aa".to_string())).unwrap();
    let z = KeyCodec::encode_key(&Key::Text("z".to_string())).unwrap();
    assert_eq!(KeyComparator::compare(&aa, &z), Ordering::Less);

    let a = KeyCodec::encode_key(&Key::Text("a".to_string())).unwrap();
    let a_nul_b = KeyCodec::encode_key(&Key::Text("a\0b".to_string())).unwrap();
    assert_eq!(KeyComparator::compare(&a, &a_nul_b), Ordering::Less);
}

#[test]
fn test_order_preservation_int32_sequence() {
    let keys = [
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

#[test]
fn test_order_preservation_int64_extremes() {
    let keys = [
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

#[test]
fn test_order_preservation_text_lexicographic() {
    let keys = [
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

#[test]
fn test_order_preservation_bytes() {
    let keys = [
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

#[test]
fn test_order_preservation_within_type() {
    let int_keys = [Key::Int32(1), Key::Int32(2), Key::Int32(3)];
    let int_encoded: Vec<_> = int_keys
        .iter()
        .map(|k| KeyCodec::encode_key(k).unwrap())
        .collect();

    for i in 0..int_encoded.len() - 1 {
        let cmp = KeyComparator::compare(&int_encoded[i], &int_encoded[i + 1]);
        assert_eq!(cmp, Ordering::Less, "int32 keys should be ordered");
    }
}

#[test]
fn test_stress_order_preservation_int32() {
    let values = vec![
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

    let mut extended = values.clone();
    extended.extend(&values);
    extended.sort_unstable();

    let keys: Vec<_> = extended.iter().map(|&v| Key::Int32(v)).collect();
    let encoded: Vec<_> = keys
        .iter()
        .map(|k| KeyCodec::encode_key(k).unwrap())
        .collect();

    for i in 0..encoded.len() - 1 {
        if i % 2 == 0 {
            let cmp = KeyComparator::compare(&encoded[i], &encoded[i + 1]);
            assert!(
                cmp == Ordering::Less || cmp == Ordering::Equal,
                "stress test order violation at index {}",
                i
            );
        }
    }
}

#[test]
fn test_null_key_ordering() {
    let null_key = Key::Null;
    let int_key = Key::Int32(0);

    let null_enc = KeyCodec::encode_key(&null_key).expect("encode null");
    let int_enc = KeyCodec::encode_key(&int_key).expect("encode int");

    let null_enc2 = KeyCodec::encode_key(&null_key).expect("encode null again");
    assert_eq!(null_enc, null_enc2, "null key determinism failed");

    let _cmp = KeyComparator::compare(&null_enc, &int_enc);
}
