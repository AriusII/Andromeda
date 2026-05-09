use andromeda_error::AndromedaResult;

use super::validation::{checked_end, checked_u16_len, codec_error};

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
    pub(crate) fn from_tag(tag: u8) -> AndromedaResult<Self> {
        match tag {
            0 => Ok(Self::Null),
            1 => Ok(Self::Int32),
            2 => Ok(Self::Int64),
            3 => Ok(Self::Text),
            4 => Ok(Self::Bytes),
            5 => Ok(Self::Composite),
            _ => Err(codec_error(format!("unknown key type tag: {}", tag))),
        }
    }

    pub(crate) fn tag(self) -> u8 {
        self as u8
    }
}

pub(crate) const DATUM_BOOL_TAG: u8 = 6;
const SIGN_BIT_MASK: u8 = 0x80;

pub(crate) fn encode_len_prefixed_key(
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

pub(crate) fn read_fixed_payload<const N: usize>(
    bytes: &[u8],
    position: usize,
    length: usize,
    invalid_length_msg: &'static str,
    truncated_msg: &'static str,
) -> AndromedaResult<[u8; N]> {
    if length != N {
        return Err(codec_error(invalid_length_msg));
    }
    read_array_at(bytes, position, truncated_msg)
}

pub(crate) fn read_u16_at(
    bytes: &[u8],
    position: usize,
    truncated_msg: &'static str,
) -> AndromedaResult<u16> {
    let end = checked_end(bytes, position, 2, truncated_msg)?;
    Ok(u16::from_le_bytes([bytes[position], bytes[end - 1]]))
}

pub(crate) fn encode_int32_order_preserving(value: i32) -> [u8; 4] {
    flip_sign_bit(value.to_be_bytes())
}

pub(crate) fn decode_int32_order_preserving(bytes: &[u8; 4]) -> i32 {
    i32::from_be_bytes(flip_sign_bit(*bytes))
}

pub(crate) fn encode_int64_order_preserving(value: i64) -> [u8; 8] {
    flip_sign_bit(value.to_be_bytes())
}

pub(crate) fn decode_int64_order_preserving(bytes: &[u8; 8]) -> i64 {
    i64::from_be_bytes(flip_sign_bit(*bytes))
}

pub(crate) fn encode_text_order_preserving(value: &str) -> Vec<u8> {
    let mut result = Vec::with_capacity(value.len() + 1);
    for byte in value.as_bytes() {
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

pub(crate) fn decode_text_order_preserving(bytes: &[u8]) -> AndromedaResult<(String, usize)> {
    let mut decoded = Vec::new();
    let mut position = 0;

    while position < bytes.len() {
        match bytes[position] {
            0 => {
                if position + 1 < bytes.len() && bytes[position + 1] == 0xff {
                    decoded.push(0);
                    position += 2;
                } else {
                    let value = String::from_utf8(decoded)
                        .map_err(|_| codec_error("invalid UTF-8 in text key"))?;
                    return Ok((value, position + 1));
                }
            },
            byte => {
                decoded.push(byte);
                position += 1;
            },
        }
    }

    Err(codec_error("truncated text: missing terminator"))
}

fn flip_sign_bit<const N: usize>(mut bytes: [u8; N]) -> [u8; N] {
    bytes[0] ^= SIGN_BIT_MASK;
    bytes
}

fn read_array_at<const N: usize>(
    bytes: &[u8],
    position: usize,
    truncated_msg: &'static str,
) -> AndromedaResult<[u8; N]> {
    let end = checked_end(bytes, position, N, truncated_msg)?;
    let mut result = [0; N];
    result.copy_from_slice(&bytes[position..end]);
    Ok(result)
}
