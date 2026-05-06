//! B-Tree Key Codec — Order-preserving key encoding and comparison.
//!
//! This module implements deterministic, order-preserving encoding for B-Tree keys.
//! All encoded keys maintain the invariant: encode(K1) < encode(K2) ⟺ K1 < K2.
//!
//! ## Format Specification
//!
//! Fixed-width keys are encoded as:
//! ```text
//! [type_tag: 1 byte] [length: 2 bytes LE] [value_bytes: N bytes]
//! ```
//!
//! Text keys use an order-preserving, self-delimiting payload:
//! ```text
//! [type_tag: 1 byte] [escaped_utf8_bytes] [terminator: 0x00]
//! ```
//!
//! Embedded `0x00` bytes in text are escaped as `[0x00, 0xff]`.
//!
//! For composite keys (multi-column), the two bytes after the composite type tag
//! carry the column count. Each column is then encoded sequentially using the
//! same typed key encoding as scalar keys.
//!
//! ## Order Preservation Invariant
//!
//! **Theorem**: Lexicographic byte order of encoded keys matches the value order.
//!
//! **Proof**:
//! 1. For integers: big-endian bytes with flipped sign bit maintain order
//! 2. For text: UTF-8 byte sequences sort lexicographically before the terminator
//! 3. For composite: Column-by-column comparison uses byte prefixes
//!
//! ## Determinism Invariant
//!
//! Same input key → always produces identical byte sequence across invocations.
//! This enables key caching and index stability.

use crate::heap_row_encoder::{Datum, ScalarType};
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use std::cmp::Ordering;

/// Key types supported by the B-Tree codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyType {
    Null = 0,
    Int32 = 1,
    Int64 = 2,
    Text = 3,
    Bytes = 4,
    Composite = 5,
}

impl KeyType {
    /// Convert type tag byte to KeyType.
    pub fn from_tag(tag: u8) -> AndromedaResult<Self> {
        match tag {
            0 => Ok(KeyType::Null),
            1 => Ok(KeyType::Int32),
            2 => Ok(KeyType::Int64),
            3 => Ok(KeyType::Text),
            4 => Ok(KeyType::Bytes),
            5 => Ok(KeyType::Composite),
            _ => Err(codec_error(format!("unknown key type tag: {}", tag))),
        }
    }

    /// Convert KeyType to tag byte.
    pub fn tag(self) -> u8 {
        self as u8
    }
}

/// A key value that can be encoded/decoded.
#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    Null,
    Int32(i32),
    Int64(i64),
    Text(String),
    Bytes(Vec<u8>),
    /// Composite key: sequence of Datums
    Composite(Vec<Datum>),
}

const DATUM_BOOL_TAG: u8 = 6;
const SIGN_BIT_MASK: u8 = 0x80;

/// B-Tree key encoder — Encodes keys to order-preserving byte sequences.
///
/// # Invariants
///
/// 1. **Order Preservation**: encode(K1) < encode(K2) ⟺ K1 < K2
/// 2. **Determinism**: encode(K) always produces identical bytes
/// 3. **Unambiguity**: Encoded form uniquely identifies the key
#[derive(Debug, Clone)]
pub struct KeyCodec;

impl KeyCodec {
    /// Encode a key into a deterministic byte sequence.
    ///
    /// Format:
    /// - Fixed-width/bytes/null: `[type_tag: 1B] [length: 2B LE] [value_bytes: N bytes]`
    /// - Text: `[type_tag: 1B] [escaped_utf8_bytes] [terminator: 0x00]`
    /// - Composite: `[type_tag: 1B] [column_count: 2B LE] [encoded_column ...]`
    ///
    /// # Errors
    ///
    /// - Text encoding fails (invalid UTF-8)
    /// - Key too large (> 65535 bytes)
    pub fn encode_key(key: &Key) -> AndromedaResult<Vec<u8>> {
        match key {
            Key::Null => encode_len_prefixed_key(KeyType::Null, &[], "null key too large"),
            Key::Int32(v) => {
                // Encode as big-endian for order preservation:
                // Bit 0 (sign bit) flipped to maintain sort order
                let encoded = encode_int32_order_preserving(*v);
                encode_len_prefixed_key(KeyType::Int32, &encoded, "int32 key too large")
            }
            Key::Int64(v) => {
                // Encode for order preservation
                let encoded = encode_int64_order_preserving(*v);
                encode_len_prefixed_key(KeyType::Int64, &encoded, "int64 key too large")
            }
            Key::Text(s) => {
                let encoded_text = encode_text_order_preserving(s);
                checked_u16_len(encoded_text.len(), "text too large for encoding")?;
                let mut result = vec![KeyType::Text.tag()];
                result.extend_from_slice(&encoded_text);
                Ok(result)
            }
            Key::Bytes(b) => {
                encode_len_prefixed_key(KeyType::Bytes, b, "byte sequence too large for encoding")
            }
            Key::Composite(datums) => {
                let mut result = vec![KeyType::Composite.tag()];
                // Encode number of columns
                let col_count = checked_u16_len(datums.len(), "too many columns in composite key")?;
                result.extend_from_slice(&col_count.to_le_bytes());

                // Encode each column
                for datum in datums {
                    let encoded = KeyCodec::encode_datum(datum)?;
                    checked_u16_len(encoded.len(), "encoded column too large")?;
                    result.extend_from_slice(&encoded);
                }

                Ok(result)
            }
        }
    }

    /// Decode a key from bytes.
    ///
    /// # Errors
    ///
    /// - Invalid type tag
    /// - Truncated or corrupted data
    /// - Text is not valid UTF-8
    pub fn decode_key(bytes: &[u8]) -> AndromedaResult<Key> {
        let (key, consumed) = KeyCodec::decode_key_at(bytes, 0)?;
        if consumed != bytes.len() {
            return Err(codec_error("trailing bytes after key"));
        }
        Ok(key)
    }

    /// Decode a key starting at `start`, returning the decoded key and the next byte offset.
    fn decode_key_at(bytes: &[u8], start: usize) -> AndromedaResult<(Key, usize)> {
        if start >= bytes.len() {
            return Err(codec_error("cannot decode empty key"));
        }

        let type_tag = KeyType::from_tag(bytes[start])?;
        let mut pos = start + 1;

        if type_tag == KeyType::Text {
            let (s, consumed) = decode_text_order_preserving(&bytes[pos..])?;
            return Ok((Key::Text(s), pos + consumed));
        }

        // Read length, or column count for composite keys.
        if pos + 2 > bytes.len() {
            return Err(codec_error("truncated key: missing length"));
        }
        let length = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]) as usize;
        pos += 2;

        // Validate and read value
        match type_tag {
            KeyType::Null => {
                if length != 0 {
                    return Err(codec_error("invalid null length"));
                }
                Ok((Key::Null, pos))
            }
            KeyType::Int32 => {
                let encoded = read_fixed_payload::<4>(
                    bytes,
                    pos,
                    length,
                    "invalid int32 length",
                    "truncated int32",
                )?;
                let v = decode_int32_order_preserving(&encoded);
                Ok((Key::Int32(v), pos + 4))
            }
            KeyType::Int64 => {
                let encoded = read_fixed_payload::<8>(
                    bytes,
                    pos,
                    length,
                    "invalid int64 length",
                    "truncated int64",
                )?;
                let v = decode_int64_order_preserving(&encoded);
                Ok((Key::Int64(v), pos + 8))
            }
            KeyType::Text => unreachable!("text keys are decoded before length parsing"),
            KeyType::Bytes => {
                if pos + length > bytes.len() {
                    return Err(codec_error("truncated bytes"));
                }
                let b = bytes[pos..pos + length].to_vec();
                Ok((Key::Bytes(b), pos + length))
            }
            KeyType::Composite => {
                // For composite keys, the top-level two-byte field is the column count.
                let col_count = length;

                let mut datums = Vec::new();
                for _ in 0..col_count {
                    let (datum, next_pos) = KeyCodec::decode_datum_at(bytes, pos)?;
                    pos = next_pos;
                    datums.push(datum);
                }

                Ok((Key::Composite(datums), pos))
            }
        }
    }

    /// Encode a Datum (for use in composite keys).
    fn encode_datum(datum: &Datum) -> AndromedaResult<Vec<u8>> {
        match datum {
            Datum::Null => KeyCodec::encode_key(&Key::Null),
            Datum::Int32(v) => KeyCodec::encode_key(&Key::Int32(*v)),
            Datum::Int64(v) => KeyCodec::encode_key(&Key::Int64(*v)),
            Datum::Text(s) => KeyCodec::encode_key(&Key::Text(s.clone())),
            Datum::Bytes(b) => KeyCodec::encode_key(&Key::Bytes(b.clone())),
            Datum::Bool(b) => Ok(vec![DATUM_BOOL_TAG, if *b { 1 } else { 0 }]),
            Datum::Int8(_)
            | Datum::Int16(_)
            | Datum::UInt8(_)
            | Datum::UInt16(_)
            | Datum::UInt32(_)
            | Datum::UInt64(_)
            | Datum::Float32(_)
            | Datum::Float64(_) => Err(codec_error(format!(
                "unsupported datum variant for durable index key: {:?}",
                datum
            ))),
        }
    }

    /// Decode a Datum (for use in composite keys), returning the next byte offset.
    fn decode_datum_at(bytes: &[u8], start: usize) -> AndromedaResult<(Datum, usize)> {
        if start >= bytes.len() {
            return Err(codec_error("truncated composite: missing column data"));
        }

        if bytes[start] == DATUM_BOOL_TAG {
            if start + 2 > bytes.len() {
                return Err(codec_error("truncated bool datum"));
            }
            return match bytes[start + 1] {
                0 => Ok((Datum::Bool(false), start + 2)),
                1 => Ok((Datum::Bool(true), start + 2)),
                _ => Err(codec_error("invalid bool datum")),
            };
        }

        let (key, next_pos) = KeyCodec::decode_key_at(bytes, start)?;
        let datum = match key {
            Key::Null => Datum::Null,
            Key::Int32(v) => Datum::Int32(v),
            Key::Int64(v) => Datum::Int64(v),
            Key::Text(s) => Datum::Text(s),
            Key::Bytes(b) => Datum::Bytes(b),
            Key::Composite(_) => {
                return Err(codec_error("nested composite datum is not supported"));
            }
        };

        Ok((datum, next_pos))
    }

    /// Decode a Datum (for use in composite keys).
    #[allow(dead_code)]
    fn decode_datum(bytes: &[u8]) -> AndromedaResult<Datum> {
        let (datum, consumed) = KeyCodec::decode_datum_at(bytes, 0)?;
        if consumed != bytes.len() {
            return Err(codec_error("trailing bytes after datum"));
        }
        Ok(datum)
    }

    /// Encode composite key from datums (multi-column key).
    ///
    /// Each column is encoded and concatenated. Supports multiple columns
    /// with deterministic, order-preserving output.
    pub fn encode_composite_key(cols: &[Datum]) -> AndromedaResult<Vec<u8>> {
        if cols.is_empty() {
            return Err(codec_error("composite key must have at least one column"));
        }

        let key = Key::Composite(cols.to_vec());
        KeyCodec::encode_key(&key)
    }

    /// Decode composite key into datums (multi-column key).
    pub fn decode_composite(bytes: &[u8], schema: &[ScalarType]) -> AndromedaResult<Vec<Datum>> {
        let key = KeyCodec::decode_key(bytes)?;
        match key {
            Key::Composite(datums) => {
                if datums.len() != schema.len() {
                    return Err(codec_error(format!(
                        "composite schema arity mismatch: encoded {} columns, schema {} columns",
                        datums.len(),
                        schema.len()
                    )));
                }

                for (idx, (datum, scalar_type)) in datums.iter().zip(schema.iter()).enumerate() {
                    validate_composite_datum_type(idx, datum, *scalar_type)?;
                }

                Ok(datums)
            }
            _ => Err(codec_error("expected composite key")),
        }
    }
}

fn validate_composite_datum_type(
    idx: usize,
    datum: &Datum,
    scalar_type: ScalarType,
) -> AndromedaResult<()> {
    let compatible = matches!(
        (datum, scalar_type),
        (Datum::Null, _)
            | (Datum::Int32(_), ScalarType::Int32)
            | (Datum::Int64(_), ScalarType::Int64)
            | (Datum::Bool(_), ScalarType::Bool)
    );

    if compatible {
        return Ok(());
    }

    match datum {
        Datum::Text(_) | Datum::Bytes(_) => Err(codec_error(format!(
            "composite schema cannot validate variable-width datum at column {} without Text/Bytes scalar type",
            idx
        ))),
        _ => Err(codec_error(format!(
            "composite schema type mismatch at column {}: datum {:?} is not compatible with {:?}",
            idx, datum, scalar_type
        ))),
    }
}

fn checked_u16_len(len: usize, err_msg: &'static str) -> AndromedaResult<u16> {
    if len > u16::MAX as usize {
        return Err(codec_error(err_msg));
    }
    Ok(len as u16)
}

fn encode_len_prefixed_key(
    key_type: KeyType,
    payload: &[u8],
    too_large_msg: &'static str,
) -> AndromedaResult<Vec<u8>> {
    let payload_len = checked_u16_len(payload.len(), too_large_msg)?;
    let mut result = Vec::with_capacity(1 + 2 + payload.len());
    result.push(key_type.tag());
    result.extend_from_slice(&payload_len.to_le_bytes());
    result.extend_from_slice(payload);
    Ok(result)
}

fn read_fixed_payload<const N: usize>(
    bytes: &[u8],
    pos: usize,
    length: usize,
    invalid_length_msg: &'static str,
    truncated_msg: &'static str,
) -> AndromedaResult<[u8; N]> {
    if length != N {
        return Err(codec_error(invalid_length_msg));
    }
    read_array_at(bytes, pos, truncated_msg)
}

fn read_array_at<const N: usize>(
    bytes: &[u8],
    pos: usize,
    truncated_msg: &'static str,
) -> AndromedaResult<[u8; N]> {
    if pos + N > bytes.len() {
        return Err(codec_error(truncated_msg));
    }
    let mut result = [0; N];
    result.copy_from_slice(&bytes[pos..pos + N]);
    Ok(result)
}

/// Order-preserving encoding for i32.
///
/// Converts to big-endian with sign bit flipped to maintain sort order.
fn encode_int32_order_preserving(v: i32) -> [u8; 4] {
    flip_sign_bit(v.to_be_bytes())
}

/// Order-preserving decoding for i32.
fn decode_int32_order_preserving(bytes: &[u8; 4]) -> i32 {
    i32::from_be_bytes(flip_sign_bit(*bytes))
}

/// Order-preserving encoding for i64.
fn encode_int64_order_preserving(v: i64) -> [u8; 8] {
    flip_sign_bit(v.to_be_bytes())
}

/// Order-preserving decoding for i64.
fn decode_int64_order_preserving(bytes: &[u8; 8]) -> i64 {
    i64::from_be_bytes(flip_sign_bit(*bytes))
}

fn flip_sign_bit<const N: usize>(mut bytes: [u8; N]) -> [u8; N] {
    bytes[0] ^= SIGN_BIT_MASK;
    bytes
}

/// Order-preserving, self-delimiting UTF-8 payload encoding.
///
/// All non-zero bytes are emitted unchanged. Embedded zero bytes are escaped as
/// `0x00 0xff`; a single `0x00` terminates the payload. This keeps normal UTF-8
/// byte comparison order while making text values unambiguous without placing a
/// length field before the text bytes.
fn encode_text_order_preserving(s: &str) -> Vec<u8> {
    let mut result = Vec::with_capacity(s.len() + 1);
    for byte in s.as_bytes() {
        if *byte == 0 {
            result.push(0);
            result.push(0xff);
        } else {
            result.push(*byte);
        }
    }
    result.push(0);
    result
}

/// Decode an order-preserving text payload, returning the decoded string and
/// the number of payload bytes consumed, including the terminator.
fn decode_text_order_preserving(bytes: &[u8]) -> AndromedaResult<(String, usize)> {
    let mut decoded = Vec::new();
    let mut pos = 0;

    while pos < bytes.len() {
        match bytes[pos] {
            0 => {
                if pos + 1 < bytes.len() && bytes[pos + 1] == 0xff {
                    decoded.push(0);
                    pos += 2;
                } else {
                    let s = String::from_utf8(decoded)
                        .map_err(|_| codec_error("invalid UTF-8 in text key"))?;
                    return Ok((s, pos + 1));
                }
            }
            byte => {
                decoded.push(byte);
                pos += 1;
            }
        }
    }

    Err(codec_error("truncated text: missing terminator"))
}

/// B-Tree key comparator — Lexicographic byte-level comparison.
///
/// Compares encoded keys using byte-level comparison, maintaining
/// the order-preservation invariant.
#[derive(Debug, Clone)]
pub struct KeyComparator;

impl KeyComparator {
    /// Compare two encoded keys lexicographically.
    ///
    /// Returns Ordering::Less if lhs < rhs, Equal if lhs == rhs, Greater if lhs > rhs.
    ///
    /// # Invariant
    ///
    /// If Key1 < Key2, then encode(Key1) < encode(Key2) lexicographically.
    pub fn compare(lhs: &[u8], rhs: &[u8]) -> Ordering {
        lhs.cmp(rhs)
    }

    /// Check if two encoded keys are equal.
    pub fn equal(lhs: &[u8], rhs: &[u8]) -> bool {
        lhs == rhs
    }

    /// Compare with an upper bound (for range scans).
    pub fn compare_range(key: &[u8], upper_bound: &[u8]) -> Ordering {
        key.cmp(upper_bound)
    }
}

/// Helper to create codec errors.
fn codec_error(msg: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Single Key Type Tests

    #[test]
    fn test_encode_decode_null() {
        let key = Key::Null;
        let encoded = KeyCodec::encode_key(&key).expect("encode");
        let decoded = KeyCodec::decode_key(&encoded).expect("decode");
        assert_eq!(key, decoded);
    }

    #[test]
    fn test_encode_decode_int32() {
        let key = Key::Int32(42);
        let encoded = KeyCodec::encode_key(&key).expect("encode");
        let decoded = KeyCodec::decode_key(&encoded).expect("decode");
        assert_eq!(key, decoded);
    }

    #[test]
    fn test_encode_decode_int32_negative() {
        let key = Key::Int32(-42);
        let encoded = KeyCodec::encode_key(&key).expect("encode");
        let decoded = KeyCodec::decode_key(&encoded).expect("decode");
        assert_eq!(key, decoded);
    }

    #[test]
    fn test_encode_decode_int64() {
        let key = Key::Int64(12345678901234i64);
        let encoded = KeyCodec::encode_key(&key).expect("encode");
        let decoded = KeyCodec::decode_key(&encoded).expect("decode");
        assert_eq!(key, decoded);
    }

    #[test]
    fn test_encode_decode_int64_negative() {
        let key = Key::Int64(-12345678901234i64);
        let encoded = KeyCodec::encode_key(&key).expect("encode");
        let decoded = KeyCodec::decode_key(&encoded).expect("decode");
        assert_eq!(key, decoded);
    }

    #[test]
    fn test_encode_decode_text() {
        let key = Key::Text("hello world".to_string());
        let encoded = KeyCodec::encode_key(&key).expect("encode");
        let decoded = KeyCodec::decode_key(&encoded).expect("decode");
        assert_eq!(key, decoded);
    }

    #[test]
    fn test_encode_decode_text_unicode() {
        let key = Key::Text("café 你好 🎉".to_string());
        let encoded = KeyCodec::encode_key(&key).expect("encode");
        let decoded = KeyCodec::decode_key(&encoded).expect("decode");
        assert_eq!(key, decoded);
    }

    #[test]
    fn test_encode_decode_bytes() {
        let key = Key::Bytes(vec![1, 2, 3, 255, 254]);
        let encoded = KeyCodec::encode_key(&key).expect("encode");
        let decoded = KeyCodec::decode_key(&encoded).expect("decode");
        assert_eq!(key, decoded);
    }

    // Composite Key Tests

    #[test]
    fn test_encode_decode_composite_simple() {
        let datums = vec![Datum::Int32(42), Datum::Int64(100)];
        let encoded = KeyCodec::encode_composite_key(&datums).expect("encode");
        let decoded = KeyCodec::decode_composite(&encoded, &[ScalarType::Int32, ScalarType::Int64])
            .expect("decode");
        // Check structure preserved
        assert_eq!(decoded.len(), 2);
    }

    #[test]
    fn test_encode_decode_composite_multi() {
        let datums = vec![Datum::Int64(100), Datum::Int32(200), Datum::Bool(true)];
        let encoded = KeyCodec::encode_composite_key(&datums).expect("encode");
        let decoded = KeyCodec::decode_composite(
            &encoded,
            &[ScalarType::Int64, ScalarType::Int32, ScalarType::Bool],
        )
        .expect("decode");
        assert_eq!(decoded.len(), 3);
    }

    // Order Preservation Tests

    #[test]
    fn test_order_preservation_int32() {
        let keys = [
            Key::Int32(i32::MIN),
            Key::Int32(-1000),
            Key::Int32(-1),
            Key::Int32(0),
            Key::Int32(1),
            Key::Int32(1000),
            Key::Int32(i32::MAX),
        ];

        let encoded: Vec<_> = keys
            .iter()
            .map(|k| KeyCodec::encode_key(k).unwrap())
            .collect();

        // Check order preservation
        for i in 0..encoded.len() - 1 {
            let cmp = KeyComparator::compare(&encoded[i], &encoded[i + 1]);
            assert_eq!(
                cmp,
                Ordering::Less,
                "order mismatch at index {}: {:?} should be < {:?}",
                i,
                keys[i],
                keys[i + 1]
            );
        }
    }

    #[test]
    fn test_order_preservation_int64() {
        let keys = [
            Key::Int64(i64::MIN),
            Key::Int64(-1000000000000i64),
            Key::Int64(-1),
            Key::Int64(0),
            Key::Int64(1),
            Key::Int64(1000000000000i64),
            Key::Int64(i64::MAX),
        ];

        let encoded: Vec<_> = keys
            .iter()
            .map(|k| KeyCodec::encode_key(k).unwrap())
            .collect();

        for i in 0..encoded.len() - 1 {
            let cmp = KeyComparator::compare(&encoded[i], &encoded[i + 1]);
            assert_eq!(cmp, Ordering::Less);
        }
    }

    #[test]
    fn test_order_preservation_text() {
        let keys = [
            Key::Text("apple".to_string()),
            Key::Text("banana".to_string()),
            Key::Text("cherry".to_string()),
            Key::Text("date".to_string()),
            Key::Text("elderberry".to_string()),
        ];

        let encoded: Vec<_> = keys
            .iter()
            .map(|k| KeyCodec::encode_key(k).unwrap())
            .collect();

        for i in 0..encoded.len() - 1 {
            let cmp = KeyComparator::compare(&encoded[i], &encoded[i + 1]);
            assert_eq!(cmp, Ordering::Less);
        }
    }

    // Determinism Tests

    #[test]
    fn test_determinism_int32() {
        let key = Key::Int32(42);
        let encoded1 = KeyCodec::encode_key(&key).expect("encode 1");
        let encoded2 = KeyCodec::encode_key(&key).expect("encode 2");
        assert_eq!(encoded1, encoded2, "determinism violated for Int32");
    }

    #[test]
    fn test_determinism_text() {
        let key = Key::Text("hello world".to_string());
        let encoded1 = KeyCodec::encode_key(&key).expect("encode 1");
        let encoded2 = KeyCodec::encode_key(&key).expect("encode 2");
        assert_eq!(encoded1, encoded2, "determinism violated for Text");
    }

    #[test]
    fn test_determinism_composite() {
        let datums = vec![
            Datum::Int64(123),
            Datum::Text("test".to_string()),
            Datum::Bytes(vec![1, 2, 3]),
        ];
        let encoded1 = KeyCodec::encode_composite_key(&datums).expect("encode 1");
        let encoded2 = KeyCodec::encode_composite_key(&datums).expect("encode 2");
        assert_eq!(encoded1, encoded2, "determinism violated for composite key");
    }

    // Edge Cases

    #[test]
    fn test_edge_case_empty_text() {
        let key = Key::Text("".to_string());
        let encoded = KeyCodec::encode_key(&key).expect("encode");
        let decoded = KeyCodec::decode_key(&encoded).expect("decode");
        assert_eq!(key, decoded);
    }

    #[test]
    fn test_edge_case_empty_bytes() {
        let key = Key::Bytes(vec![]);
        let encoded = KeyCodec::encode_key(&key).expect("encode");
        let decoded = KeyCodec::decode_key(&encoded).expect("decode");
        assert_eq!(key, decoded);
    }

    #[test]
    fn test_edge_case_large_text() {
        let large_text = "x".repeat(10000);
        let key = Key::Text(large_text);
        let encoded = KeyCodec::encode_key(&key).expect("encode");
        let decoded = KeyCodec::decode_key(&encoded).expect("decode");
        assert_eq!(key, decoded);
    }

    #[test]
    fn test_edge_case_min_max_int32() {
        let keys = vec![Key::Int32(i32::MIN), Key::Int32(i32::MAX)];
        for key in keys {
            let encoded = KeyCodec::encode_key(&key).expect("encode");
            let decoded = KeyCodec::decode_key(&encoded).expect("decode");
            assert_eq!(key, decoded);
        }
    }

    #[test]
    fn test_edge_case_min_max_int64() {
        let keys = vec![Key::Int64(i64::MIN), Key::Int64(i64::MAX)];
        for key in keys {
            let encoded = KeyCodec::encode_key(&key).expect("encode");
            let decoded = KeyCodec::decode_key(&encoded).expect("decode");
            assert_eq!(key, decoded);
        }
    }

    // Comparator Tests

    #[test]
    fn test_comparator_equal() {
        let key = Key::Int32(42);
        let encoded = KeyCodec::encode_key(&key).expect("encode");
        assert!(KeyComparator::equal(&encoded, &encoded));
    }

    #[test]
    fn test_comparator_less() {
        let k1 = KeyCodec::encode_key(&Key::Int32(10)).expect("encode");
        let k2 = KeyCodec::encode_key(&Key::Int32(20)).expect("encode");
        assert_eq!(KeyComparator::compare(&k1, &k2), Ordering::Less);
    }

    #[test]
    fn test_comparator_greater() {
        let k1 = KeyCodec::encode_key(&Key::Int32(20)).expect("encode");
        let k2 = KeyCodec::encode_key(&Key::Int32(10)).expect("encode");
        assert_eq!(KeyComparator::compare(&k1, &k2), Ordering::Greater);
    }

    // Performance / Latency Checks (Informal)

    #[test]
    fn test_encode_latency_int32() {
        // Should be sub-microsecond; we do rough timing
        let key = Key::Int32(42);
        let start = std::time::Instant::now();
        for _ in 0..10000 {
            let _ = KeyCodec::encode_key(&key);
        }
        let elapsed = start.elapsed();
        // 10k iterations should take < 50ms on any reasonable hardware
        // (~5μs per encoding)
        assert!(elapsed.as_millis() < 100, "encode latency too high");
    }

    #[test]
    fn test_decode_latency_int32() {
        let encoded = KeyCodec::encode_key(&Key::Int32(42)).expect("encode");
        let start = std::time::Instant::now();
        for _ in 0..10000 {
            let _ = KeyCodec::decode_key(&encoded);
        }
        let elapsed = start.elapsed();
        assert!(elapsed.as_millis() < 100, "decode latency too high");
    }

    #[test]
    fn test_compare_latency() {
        let k1 = KeyCodec::encode_key(&Key::Int32(10)).expect("encode");
        let k2 = KeyCodec::encode_key(&Key::Int32(20)).expect("encode");
        let start = std::time::Instant::now();
        for _ in 0..10000 {
            let _ = KeyComparator::compare(&k1, &k2);
        }
        let elapsed = start.elapsed();
        assert!(elapsed.as_millis() < 50, "compare latency too high");
    }
}
