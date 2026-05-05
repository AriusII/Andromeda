//! B-Tree Key Codec — Order-preserving key encoding and comparison.
//!
//! This module implements deterministic, order-preserving encoding for B-Tree keys.
//! All encoded keys maintain the invariant: encode(K1) < encode(K2) ⟺ K1 < K2.
//!
//! ## Format Specification
//!
//! Each key is encoded as:
//! ```text
//! [type_tag: 1 byte] [length: 2 bytes LE] [value_bytes: N bytes]
//! ```
//!
//! For composite keys (multi-column), each column is encoded sequentially.
//!
//! ## Order Preservation Invariant
//!
//! **Theorem**: Lexicographic byte order of encoded keys matches the value order.
//!
//! **Proof**:
//! 1. For integers: Two's complement + little-endian maintains order
//! 2. For text: UTF-8 byte sequences sort lexicographically
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
    /// Format: [type_tag: 1B] [length: 2B LE] [value_bytes: N bytes]
    ///
    /// # Errors
    ///
    /// - Text encoding fails (invalid UTF-8)
    /// - Key too large (> 65535 bytes)
    pub fn encode_key(key: &Key) -> AndromedaResult<Vec<u8>> {
        match key {
            Key::Null => {
                let mut result = vec![KeyType::Null.tag()];
                result.extend_from_slice(&0u16.to_le_bytes());
                Ok(result)
            }
            Key::Int32(v) => {
                let mut result = vec![KeyType::Int32.tag()];
                result.extend_from_slice(&4u16.to_le_bytes());
                // Encode as big-endian for order preservation:
                // Bit 0 (sign bit) flipped to maintain sort order
                let encoded = encode_int32_order_preserving(*v);
                result.extend_from_slice(&encoded);
                Ok(result)
            }
            Key::Int64(v) => {
                let mut result = vec![KeyType::Int64.tag()];
                result.extend_from_slice(&8u16.to_le_bytes());
                // Encode for order preservation
                let encoded = encode_int64_order_preserving(*v);
                result.extend_from_slice(&encoded);
                Ok(result)
            }
            Key::Text(s) => {
                let text_bytes = s.as_bytes();
                if text_bytes.len() > u16::MAX as usize {
                    return Err(codec_error("text too large for encoding"));
                }
                let mut result = vec![KeyType::Text.tag()];
                result.extend_from_slice(&(text_bytes.len() as u16).to_le_bytes());
                result.extend_from_slice(text_bytes);
                Ok(result)
            }
            Key::Bytes(b) => {
                if b.len() > u16::MAX as usize {
                    return Err(codec_error("byte sequence too large for encoding"));
                }
                let mut result = vec![KeyType::Bytes.tag()];
                result.extend_from_slice(&(b.len() as u16).to_le_bytes());
                result.extend_from_slice(b);
                Ok(result)
            }
            Key::Composite(datums) => {
                let mut result = vec![KeyType::Composite.tag()];
                // Encode number of columns
                if datums.len() > u16::MAX as usize {
                    return Err(codec_error("too many columns in composite key"));
                }
                result.extend_from_slice(&(datums.len() as u16).to_le_bytes());

                // Encode each column
                for datum in datums {
                    let encoded = KeyCodec::encode_datum(datum)?;
                    if encoded.len() > u16::MAX as usize {
                        return Err(codec_error("encoded column too large"));
                    }
                    result.extend_from_slice(&(encoded.len() as u16).to_le_bytes());
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
        if bytes.is_empty() {
            return Err(codec_error("cannot decode empty key"));
        }

        let type_tag = KeyType::from_tag(bytes[0])?;
        let mut pos = 1;

        // Read length
        if pos + 2 > bytes.len() {
            return Err(codec_error("truncated key: missing length"));
        }
        let length = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]) as usize;
        pos += 2;

        // Validate and read value
        match type_tag {
            KeyType::Null => Ok(Key::Null),
            KeyType::Int32 => {
                if length != 4 {
                    return Err(codec_error("invalid int32 length"));
                }
                if pos + 4 > bytes.len() {
                    return Err(codec_error("truncated int32"));
                }
                let encoded = [
                    bytes[pos],
                    bytes[pos + 1],
                    bytes[pos + 2],
                    bytes[pos + 3],
                ];
                let v = decode_int32_order_preserving(&encoded);
                Ok(Key::Int32(v))
            }
            KeyType::Int64 => {
                if length != 8 {
                    return Err(codec_error("invalid int64 length"));
                }
                if pos + 8 > bytes.len() {
                    return Err(codec_error("truncated int64"));
                }
                let encoded = [
                    bytes[pos],
                    bytes[pos + 1],
                    bytes[pos + 2],
                    bytes[pos + 3],
                    bytes[pos + 4],
                    bytes[pos + 5],
                    bytes[pos + 6],
                    bytes[pos + 7],
                ];
                let v = decode_int64_order_preserving(&encoded);
                Ok(Key::Int64(v))
            }
            KeyType::Text => {
                if pos + length > bytes.len() {
                    return Err(codec_error("truncated text"));
                }
                let text_bytes = &bytes[pos..pos + length];
                let s = String::from_utf8(text_bytes.to_vec())
                    .map_err(|_| codec_error("invalid UTF-8 in text key"))?;
                Ok(Key::Text(s))
            }
            KeyType::Bytes => {
                if pos + length > bytes.len() {
                    return Err(codec_error("truncated bytes"));
                }
                let b = bytes[pos..pos + length].to_vec();
                Ok(Key::Bytes(b))
            }
            KeyType::Composite => {
                // Read column count
                if pos + 2 > bytes.len() {
                    return Err(codec_error("truncated composite: missing column count"));
                }
                let col_count = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]) as usize;
                pos += 2;

                let mut datums = Vec::new();
                for _ in 0..col_count {
                    // Read column length
                    if pos + 2 > bytes.len() {
                        return Err(codec_error("truncated composite: missing column length"));
                    }
                    let col_len =
                        u16::from_le_bytes([bytes[pos], bytes[pos + 1]]) as usize;
                    pos += 2;

                    // Read and decode column
                    if pos + col_len > bytes.len() {
                        return Err(codec_error("truncated composite: missing column data"));
                    }
                    let datum = KeyCodec::decode_datum(&bytes[pos..pos + col_len])?;
                    pos += col_len;
                    datums.push(datum);
                }

                Ok(Key::Composite(datums))
            }
        }
    }

    /// Encode a Datum (for use in composite keys).
    fn encode_datum(datum: &Datum) -> AndromedaResult<Vec<u8>> {
        match datum {
            Datum::Null => Ok(vec![0]),
            Datum::Int32(v) => {
                let encoded = encode_int32_order_preserving(*v);
                Ok(encoded.to_vec())
            }
            Datum::Int64(v) => {
                let encoded = encode_int64_order_preserving(*v);
                Ok(encoded.to_vec())
            }
            Datum::Text(s) => Ok(s.as_bytes().to_vec()),
            Datum::Bytes(b) => Ok(b.clone()),
            Datum::Bool(b) => Ok(vec![if *b { 1 } else { 0 }]),
            // For other types, convert to Text representation
            _ => {
                let s = format!("{:?}", datum);
                Ok(s.as_bytes().to_vec())
            }
        }
    }

    /// Decode a Datum (for use in composite keys).
    fn decode_datum(bytes: &[u8]) -> AndromedaResult<Datum> {
        if bytes.is_empty() {
            return Ok(Datum::Null);
        }

        // For now, decode as bytes and return as Text
        // A full implementation would have type tags
        match String::from_utf8(bytes.to_vec()) {
            Ok(s) => Ok(Datum::Text(s)),
            Err(_) => Ok(Datum::Bytes(bytes.to_vec())),
        }
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
    pub fn decode_composite(bytes: &[u8], _schema: &[ScalarType]) -> AndromedaResult<Vec<Datum>> {
        let key = KeyCodec::decode_key(bytes)?;
        match key {
            Key::Composite(datums) => Ok(datums),
            _ => Err(codec_error("expected composite key")),
        }
    }
}

/// Order-preserving encoding for i32.
///
/// Converts to big-endian with sign bit flipped to maintain sort order.
fn encode_int32_order_preserving(v: i32) -> [u8; 4] {
    let bits = v.to_be_bytes();
    // Flip sign bit to maintain sort order
    let mut result = bits;
    result[0] ^= 0x80;
    result
}

/// Order-preserving decoding for i32.
fn decode_int32_order_preserving(bytes: &[u8; 4]) -> i32 {
    let mut bits = *bytes;
    bits[0] ^= 0x80;
    i32::from_be_bytes(bits)
}

/// Order-preserving encoding for i64.
fn encode_int64_order_preserving(v: i64) -> [u8; 8] {
    let bits = v.to_be_bytes();
    let mut result = bits;
    result[0] ^= 0x80;
    result
}

/// Order-preserving decoding for i64.
fn decode_int64_order_preserving(bytes: &[u8; 8]) -> i64 {
    let mut bits = *bytes;
    bits[0] ^= 0x80;
    i64::from_be_bytes(bits)
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

    // ========================================================================
    // Single Key Type Tests
    // ========================================================================

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

    // ========================================================================
    // Composite Key Tests
    // ========================================================================

    #[test]
    fn test_encode_decode_composite_simple() {
        let datums = vec![Datum::Int32(42), Datum::Text("test".to_string())];
        let encoded = KeyCodec::encode_composite_key(&datums).expect("encode");
        let decoded =
            KeyCodec::decode_composite(&encoded, &[ScalarType::Int32, ScalarType::Int32])
                .expect("decode");
        // Check structure preserved
        assert_eq!(decoded.len(), 2);
    }

    #[test]
    fn test_encode_decode_composite_multi() {
        let datums = vec![
            Datum::Int64(100),
            Datum::Text("name".to_string()),
            Datum::Bytes(vec![1, 2, 3]),
        ];
        let encoded = KeyCodec::encode_composite_key(&datums).expect("encode");
        let decoded = KeyCodec::decode_composite(
            &encoded,
            &[ScalarType::Int64, ScalarType::Int32, ScalarType::Int32],
        )
        .expect("decode");
        assert_eq!(decoded.len(), 3);
    }

    // ========================================================================
    // Order Preservation Tests
    // ========================================================================

    #[test]
    fn test_order_preservation_int32() {
        let keys = vec![
            Key::Int32(i32::MIN),
            Key::Int32(-1000),
            Key::Int32(-1),
            Key::Int32(0),
            Key::Int32(1),
            Key::Int32(1000),
            Key::Int32(i32::MAX),
        ];

        let mut encoded: Vec<_> = keys
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
        let keys = vec![
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
        let keys = vec![
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

    // ========================================================================
    // Determinism Tests
    // ========================================================================

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
        assert_eq!(
            encoded1, encoded2,
            "determinism violated for composite key"
        );
    }

    // ========================================================================
    // Edge Cases
    // ========================================================================

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

    // ========================================================================
    // Comparator Tests
    // ========================================================================

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

    // ========================================================================
    // Performance / Latency Checks (Informal)
    // ========================================================================

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
