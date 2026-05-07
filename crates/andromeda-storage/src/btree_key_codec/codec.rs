use andromeda_core::AndromedaResult;

use crate::heap_row_encoder::{Datum, ScalarType};

use super::{
    Key,
    primitive::{
        DATUM_BOOL_TAG, KeyType, decode_int32_order_preserving, decode_int64_order_preserving,
        decode_text_order_preserving, encode_int32_order_preserving, encode_int64_order_preserving,
        encode_len_prefixed_key, encode_text_order_preserving, read_fixed_payload, read_u16_at,
    },
    validation::{checked_end, checked_u16_len, codec_error, validate_composite_datum_type},
};

/// B-Tree key encoder - encodes keys to order-preserving byte sequences.
///
/// # Invariants
///
/// 1. Order preservation: encode(K1) < encode(K2) iff K1 < K2.
/// 2. Determinism: encode(K) always produces identical bytes.
/// 3. Unambiguity: encoded form uniquely identifies the key.
#[derive(Debug, Clone)]
pub struct KeyCodec;

impl KeyCodec {
    /// Encode a key into a deterministic byte sequence.
    ///
    /// Format:
    /// - Fixed-width/bytes/null: `[type_tag: 1B] [length: 2B LE] [value_bytes: N bytes]`.
    /// - Text: `[type_tag: 1B] [escaped_utf8_bytes] [terminator: 0x00]`.
    /// - Composite: `[type_tag: 1B] [column_count: 2B LE] [encoded_column ...]`.
    pub fn encode_key(key: &Key) -> AndromedaResult<Vec<u8>> {
        match key {
            Key::Null => encode_len_prefixed_key(KeyType::Null, &[], "null key too large"),
            Key::Int32(v) => {
                let encoded = encode_int32_order_preserving(*v);
                encode_len_prefixed_key(KeyType::Int32, &encoded, "int32 key too large")
            }
            Key::Int64(v) => {
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
                let col_count = checked_u16_len(datums.len(), "too many columns in composite key")?;
                result.extend_from_slice(&col_count.to_le_bytes());

                for datum in datums {
                    let encoded = Self::encode_datum(datum)?;
                    checked_u16_len(encoded.len(), "encoded column too large")?;
                    result.extend_from_slice(&encoded);
                }

                Ok(result)
            }
        }
    }

    /// Decode a key from bytes.
    pub fn decode_key(bytes: &[u8]) -> AndromedaResult<Key> {
        let (key, consumed) = Self::decode_key_at(bytes, 0)?;
        if consumed != bytes.len() {
            return Err(codec_error("trailing bytes after key"));
        }
        Ok(key)
    }

    fn decode_key_at(bytes: &[u8], start: usize) -> AndromedaResult<(Key, usize)> {
        if start >= bytes.len() {
            return Err(codec_error("cannot decode empty key"));
        }

        let type_tag = KeyType::from_tag(bytes[start])?;
        let mut pos = start
            .checked_add(1)
            .ok_or_else(|| codec_error("key offset overflow"))?;

        if type_tag == KeyType::Text {
            let (s, consumed) = decode_text_order_preserving(&bytes[pos..])?;
            return Ok((
                Key::Text(s),
                pos.checked_add(consumed)
                    .ok_or_else(|| codec_error("text key offset overflow"))?,
            ));
        }

        let length = usize::from(read_u16_at(bytes, pos, "truncated key: missing length")?);
        pos = pos
            .checked_add(2)
            .ok_or_else(|| codec_error("key length offset overflow"))?;

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
            KeyType::Text => Err(codec_error(
                "text key unexpectedly reached length-prefixed decoder",
            )),
            KeyType::Bytes => {
                let end = checked_end(bytes, pos, length, "truncated bytes")?;
                let b = bytes[pos..end].to_vec();
                Ok((Key::Bytes(b), end))
            }
            KeyType::Composite => {
                let col_count = length;
                if col_count > bytes.len().saturating_sub(pos) {
                    return Err(codec_error(
                        "composite column count exceeds remaining encoded bytes",
                    ));
                }

                let mut datums = Vec::new();
                for _ in 0..col_count {
                    let (datum, next_pos) = Self::decode_datum_at(bytes, pos)?;
                    pos = next_pos;
                    datums.push(datum);
                }

                Ok((Key::Composite(datums), pos))
            }
        }
    }

    fn encode_datum(datum: &Datum) -> AndromedaResult<Vec<u8>> {
        match datum {
            Datum::Null => Self::encode_key(&Key::Null),
            Datum::Int32(v) => Self::encode_key(&Key::Int32(*v)),
            Datum::Int64(v) => Self::encode_key(&Key::Int64(*v)),
            Datum::Text(s) => Self::encode_key(&Key::Text(s.clone())),
            Datum::Bytes(b) => Self::encode_key(&Key::Bytes(b.clone())),
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

    fn decode_datum_at(bytes: &[u8], start: usize) -> AndromedaResult<(Datum, usize)> {
        if start >= bytes.len() {
            return Err(codec_error("truncated composite: missing column data"));
        }

        if bytes[start] == DATUM_BOOL_TAG {
            let end = checked_end(bytes, start, 2, "truncated bool datum")?;
            return match bytes[start + 1] {
                0 => Ok((Datum::Bool(false), end)),
                1 => Ok((Datum::Bool(true), end)),
                _ => Err(codec_error("invalid bool datum")),
            };
        }

        let (key, next_pos) = Self::decode_key_at(bytes, start)?;
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

    /// Encode composite key from datums.
    pub fn encode_composite_key(cols: &[Datum]) -> AndromedaResult<Vec<u8>> {
        if cols.is_empty() {
            return Err(codec_error("composite key must have at least one column"));
        }

        let key = Key::Composite(cols.to_vec());
        Self::encode_key(&key)
    }

    /// Decode composite key into datums and validate it against the supplied schema.
    pub fn decode_composite(bytes: &[u8], schema: &[ScalarType]) -> AndromedaResult<Vec<Datum>> {
        let key = Self::decode_key(bytes)?;
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
