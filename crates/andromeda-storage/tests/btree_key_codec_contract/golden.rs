use andromeda_storage::Datum;
use andromeda_storage::btree_key_codec::Key;

use crate::support::assert_golden;

#[test]
fn dec032_keyv1_golden_bytes_null() {
    assert_golden(Key::Null, &[0x00, 0x00, 0x00]);
}

#[test]
fn dec032_keyv1_golden_bytes_int32_boundaries() {
    let cases = [
        (i32::MIN, &[0x01, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00][..]),
        (-1, &[0x01, 0x04, 0x00, 0x7f, 0xff, 0xff, 0xff][..]),
        (0, &[0x01, 0x04, 0x00, 0x80, 0x00, 0x00, 0x00][..]),
        (1, &[0x01, 0x04, 0x00, 0x80, 0x00, 0x00, 0x01][..]),
        (i32::MAX, &[0x01, 0x04, 0x00, 0xff, 0xff, 0xff, 0xff][..]),
    ];

    for (value, expected) in cases {
        assert_golden(Key::Int32(value), expected);
    }
}

#[test]
fn dec032_keyv1_golden_bytes_int64_examples() {
    let cases = [
        (
            i64::MIN,
            &[
                0x02, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ][..],
        ),
        (
            -1,
            &[
                0x02, 0x08, 0x00, 0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            ][..],
        ),
        (
            0,
            &[
                0x02, 0x08, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ][..],
        ),
        (
            i64::MAX,
            &[
                0x02, 0x08, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            ][..],
        ),
    ];

    for (value, expected) in cases {
        assert_golden(Key::Int64(value), expected);
    }
}

#[test]
fn dec032_keyv1_golden_bytes_text_examples() {
    let cases = [
        ("", vec![0x03, 0x00]),
        ("a", vec![0x03, b'a', 0x00]),
        ("aa", vec![0x03, b'a', b'a', 0x00]),
        ("a\0b", vec![0x03, b'a', 0x00, 0xff, b'b', 0x00]),
        ("é", vec![0x03, 0xc3, 0xa9, 0x00]),
        ("世界", vec![0x03, 0xe4, 0xb8, 0x96, 0xe7, 0x95, 0x8c, 0x00]),
    ];

    for (value, expected) in cases {
        assert_golden(Key::Text(value.to_string()), &expected);
    }
}

#[test]
fn dec032_keyv1_golden_bytes_bytes_examples() {
    let cases = [
        (Vec::new(), vec![0x04, 0x00, 0x00]),
        (vec![0x00], vec![0x04, 0x01, 0x00, 0x00]),
        (
            vec![0x00, 0x01, 0xff],
            vec![0x04, 0x03, 0x00, 0x00, 0x01, 0xff],
        ),
    ];

    for (value, expected) in cases {
        assert_golden(Key::Bytes(value), &expected);
    }
}

#[test]
fn dec032_keyv1_golden_bytes_composite_fixed_arity() {
    let key = Key::Composite(vec![
        Datum::Int32(7),
        Datum::Text("a".to_string()),
        Datum::Null,
        Datum::Bool(true),
    ]);

    assert_golden(
        key,
        &[
            0x05, 0x04, 0x00, 0x01, 0x04, 0x00, 0x80, 0x00, 0x00, 0x07, 0x03, b'a', 0x00, 0x00,
            0x00, 0x00, 0x06, 0x01,
        ],
    );
}
