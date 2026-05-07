use andromeda_core::AndromedaResult;

use super::validation::{checked_end, checked_u16_len, codec_error};

/// Key types supported by the B-Tree codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyType {
    Null = 0,
    Int32 = 1,
    Int64 = 2,
    Text = 3,
    Bytes = 4,
    Composite = 5,
}

impl KeyType {
    /// Convert type tag byte to KeyType.
    pub(crate) fn from_tag(tag: u8) -> AndromedaResult<Self> {
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
    pub(crate) fn tag(self) -> u8 {
        self as u8
    }
}

pub(super) const DATUM_BOOL_TAG: u8 = 6;
const SIGN_BIT_MASK: u8 = 0x80;

pub(super) fn encode_len_prefixed_key(
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

pub(super) fn read_fixed_payload<const N: usize>(
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

pub(super) fn read_u16_at(
    bytes: &[u8],
    pos: usize,
    truncated_msg: &'static str,
) -> AndromedaResult<u16> {
    let end = checked_end(bytes, pos, 2, truncated_msg)?;
    Ok(u16::from_le_bytes([bytes[pos], bytes[end - 1]]))
}

pub(super) fn encode_int32_order_preserving(v: i32) -> [u8; 4] {
    flip_sign_bit(v.to_be_bytes())
}

pub(super) fn decode_int32_order_preserving(bytes: &[u8; 4]) -> i32 {
    i32::from_be_bytes(flip_sign_bit(*bytes))
}

pub(super) fn encode_int64_order_preserving(v: i64) -> [u8; 8] {
    flip_sign_bit(v.to_be_bytes())
}

pub(super) fn decode_int64_order_preserving(bytes: &[u8; 8]) -> i64 {
    i64::from_be_bytes(flip_sign_bit(*bytes))
}

fn flip_sign_bit<const N: usize>(mut bytes: [u8; N]) -> [u8; N] {
    bytes[0] ^= SIGN_BIT_MASK;
    bytes
}

pub(super) fn encode_text_order_preserving(s: &str) -> Vec<u8> {
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

pub(super) fn decode_text_order_preserving(bytes: &[u8]) -> AndromedaResult<(String, usize)> {
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

fn read_array_at<const N: usize>(
    bytes: &[u8],
    pos: usize,
    truncated_msg: &'static str,
) -> AndromedaResult<[u8; N]> {
    let end = checked_end(bytes, pos, N, truncated_msg)?;
    let mut result = [0; N];
    result.copy_from_slice(&bytes[pos..end]);
    Ok(result)
}
