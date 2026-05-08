use andromeda_core::AndromedaResult;

use super::{
    Key, KeyCodec, KeyDatum, KeyScalarType,
    primitive::{
        DATUM_BOOL_TAG, KeyType, decode_int32_order_preserving, decode_int64_order_preserving,
        decode_text_order_preserving, encode_int32_order_preserving, encode_int64_order_preserving,
        encode_len_prefixed_key, encode_text_order_preserving, read_fixed_payload, read_u16_at,
    },
    validation::{checked_end, checked_u16_len, codec_error, validate_composite_datum_type},
};

impl KeyCodec {
    pub fn encode_key(key: &Key) -> AndromedaResult<Vec<u8>> {
        match key {
            Key::Null => encode_len_prefixed_key(KeyType::Null, &[], "null key too large"),
            Key::Int32(value) => {
                let encoded = encode_int32_order_preserving(*value);
                encode_len_prefixed_key(KeyType::Int32, &encoded, "int32 key too large")
            },
            Key::Int64(value) => {
                let encoded = encode_int64_order_preserving(*value);
                encode_len_prefixed_key(KeyType::Int64, &encoded, "int64 key too large")
            },
            Key::Text(value) => {
                let encoded_text = encode_text_order_preserving(value);
                checked_u16_len(encoded_text.len(), "text too large for encoding")?;
                let mut result = vec![KeyType::Text.tag()];
                result.extend_from_slice(&encoded_text);
                Ok(result)
            },
            Key::Bytes(bytes) => encode_len_prefixed_key(
                KeyType::Bytes,
                bytes,
                "byte sequence too large for encoding",
            ),
            Key::Composite(datums) => {
                let mut result = vec![KeyType::Composite.tag()];
                let column_count =
                    checked_u16_len(datums.len(), "too many columns in composite key")?;
                result.extend_from_slice(&column_count.to_le_bytes());

                for datum in datums {
                    let encoded = Self::encode_datum(datum)?;
                    checked_u16_len(encoded.len(), "encoded column too large")?;
                    result.extend_from_slice(&encoded);
                }

                Ok(result)
            },
        }
    }

    pub fn decode_key(bytes: &[u8]) -> AndromedaResult<Key> {
        let (key, consumed) = Self::decode_key_at(bytes, 0)?;
        if consumed != bytes.len() {
            return Err(codec_error("trailing bytes after key"));
        }
        Ok(key)
    }

    pub fn encode_composite_key(columns: &[KeyDatum]) -> AndromedaResult<Vec<u8>> {
        if columns.is_empty() {
            return Err(codec_error("composite key must have at least one column"));
        }

        Self::encode_key(&Key::Composite(columns.to_vec()))
    }

    pub fn decode_composite(
        bytes: &[u8],
        schema: &[KeyScalarType],
    ) -> AndromedaResult<Vec<KeyDatum>> {
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

                for (index, (datum, scalar_type)) in datums.iter().zip(schema.iter()).enumerate() {
                    validate_composite_datum_type(index, datum, *scalar_type)?;
                }

                Ok(datums)
            },
            _ => Err(codec_error("expected composite key")),
        }
    }

    fn decode_key_at(bytes: &[u8], start: usize) -> AndromedaResult<(Key, usize)> {
        if start >= bytes.len() {
            return Err(codec_error("cannot decode empty key"));
        }

        let key_type = KeyType::from_tag(bytes[start])?;
        let mut position = start
            .checked_add(1)
            .ok_or_else(|| codec_error("key offset overflow"))?;

        if key_type == KeyType::Text {
            let (value, consumed) = decode_text_order_preserving(&bytes[position..])?;
            return Ok((
                Key::Text(value),
                position
                    .checked_add(consumed)
                    .ok_or_else(|| codec_error("text key offset overflow"))?,
            ));
        }

        let length = usize::from(read_u16_at(
            bytes,
            position,
            "truncated key: missing length",
        )?);
        position = position
            .checked_add(2)
            .ok_or_else(|| codec_error("key length offset overflow"))?;

        match key_type {
            KeyType::Null => {
                if length != 0 {
                    return Err(codec_error("invalid null length"));
                }
                Ok((Key::Null, position))
            },
            KeyType::Int32 => {
                let encoded = read_fixed_payload::<4>(
                    bytes,
                    position,
                    length,
                    "invalid int32 length",
                    "truncated int32",
                )?;
                Ok((
                    Key::Int32(decode_int32_order_preserving(&encoded)),
                    position + 4,
                ))
            },
            KeyType::Int64 => {
                let encoded = read_fixed_payload::<8>(
                    bytes,
                    position,
                    length,
                    "invalid int64 length",
                    "truncated int64",
                )?;
                Ok((
                    Key::Int64(decode_int64_order_preserving(&encoded)),
                    position + 8,
                ))
            },
            KeyType::Text => Err(codec_error(
                "text key unexpectedly reached length-prefixed decoder",
            )),
            KeyType::Bytes => {
                let end = checked_end(bytes, position, length, "truncated bytes")?;
                Ok((Key::Bytes(bytes[position..end].to_vec()), end))
            },
            KeyType::Composite => {
                let column_count = length;
                if column_count > bytes.len().saturating_sub(position) {
                    return Err(codec_error(
                        "composite column count exceeds remaining encoded bytes",
                    ));
                }

                let mut datums = Vec::new();
                for _ in 0..column_count {
                    let (datum, next_position) = Self::decode_datum_at(bytes, position)?;
                    position = next_position;
                    datums.push(datum);
                }

                Ok((Key::Composite(datums), position))
            },
        }
    }

    fn encode_datum(datum: &KeyDatum) -> AndromedaResult<Vec<u8>> {
        match datum {
            KeyDatum::Null => Self::encode_key(&Key::Null),
            KeyDatum::Int32(value) => Self::encode_key(&Key::Int32(*value)),
            KeyDatum::Int64(value) => Self::encode_key(&Key::Int64(*value)),
            KeyDatum::Text(value) => Self::encode_key(&Key::Text(value.clone())),
            KeyDatum::Bytes(value) => Self::encode_key(&Key::Bytes(value.clone())),
            KeyDatum::Bool(value) => Ok(vec![DATUM_BOOL_TAG, if *value { 1 } else { 0 }]),
            KeyDatum::Int8(_)
            | KeyDatum::Int16(_)
            | KeyDatum::UInt8(_)
            | KeyDatum::UInt16(_)
            | KeyDatum::UInt32(_)
            | KeyDatum::UInt64(_)
            | KeyDatum::Float32(_)
            | KeyDatum::Float64(_) => Err(codec_error(format!(
                "unsupported datum variant for durable index key: {:?}",
                datum
            ))),
        }
    }

    fn decode_datum_at(bytes: &[u8], start: usize) -> AndromedaResult<(KeyDatum, usize)> {
        if start >= bytes.len() {
            return Err(codec_error("truncated composite: missing column data"));
        }

        if bytes[start] == DATUM_BOOL_TAG {
            let end = checked_end(bytes, start, 2, "truncated bool datum")?;
            return match bytes[start + 1] {
                0 => Ok((KeyDatum::Bool(false), end)),
                1 => Ok((KeyDatum::Bool(true), end)),
                _ => Err(codec_error("invalid bool datum")),
            };
        }

        let (key, next_position) = Self::decode_key_at(bytes, start)?;
        let datum = match key {
            Key::Null => KeyDatum::Null,
            Key::Int32(value) => KeyDatum::Int32(value),
            Key::Int64(value) => KeyDatum::Int64(value),
            Key::Text(value) => KeyDatum::Text(value),
            Key::Bytes(value) => KeyDatum::Bytes(value),
            Key::Composite(_) => {
                return Err(codec_error("nested composite datum is not supported"));
            },
        };

        Ok((datum, next_position))
    }
}
