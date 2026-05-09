use andromeda_error::AndromedaResult;

use super::binary::{read_scalar_array, read_scalar_byte};
use super::error::encoder_error;
use super::schema::ScalarType;

/// A datum value — atomic unit of data in a tuple.
#[derive(Debug, Clone, PartialEq)]
pub enum Datum {
    Null,
    Int8(i8),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    UInt8(u8),
    UInt16(u16),
    UInt32(u32),
    UInt64(u64),
    Float32(f32),
    Float64(f64),
    Bool(bool),
    Bytes(Vec<u8>),
    Text(String),
}

impl Datum {
    /// Get byte length when serialized (excluding null bitmap and variable-length offsets).
    pub fn byte_length(&self) -> AndromedaResult<usize> {
        match self {
            Self::Null => Ok(0),
            Self::Int8(_) => Ok(1),
            Self::Int16(_) => Ok(2),
            Self::Int32(_) => Ok(4),
            Self::Int64(_) => Ok(8),
            Self::UInt8(_) => Ok(1),
            Self::UInt16(_) => Ok(2),
            Self::UInt32(_) => Ok(4),
            Self::UInt64(_) => Ok(8),
            Self::Float32(_) => Ok(4),
            Self::Float64(_) => Ok(8),
            Self::Bool(_) => Ok(1),
            Self::Bytes(b) => {
                if b.len() > u32::MAX as usize {
                    return Err(encoder_error("byte array too large"));
                }
                Ok(b.len())
            },
            Self::Text(s) => {
                let bytes = s.as_bytes();
                if bytes.len() > u32::MAX as usize {
                    return Err(encoder_error("text too large"));
                }
                Ok(bytes.len())
            },
        }
    }

    /// Encode to little-endian bytes.
    pub fn encode(&self) -> AndromedaResult<Vec<u8>> {
        match self {
            Self::Null => Ok(Vec::new()),
            Self::Int8(v) => Ok(vec![*v as u8]),
            Self::Int16(v) => Ok(v.to_le_bytes().to_vec()),
            Self::Int32(v) => Ok(v.to_le_bytes().to_vec()),
            Self::Int64(v) => Ok(v.to_le_bytes().to_vec()),
            Self::UInt8(v) => Ok(vec![*v]),
            Self::UInt16(v) => Ok(v.to_le_bytes().to_vec()),
            Self::UInt32(v) => Ok(v.to_le_bytes().to_vec()),
            Self::UInt64(v) => Ok(v.to_le_bytes().to_vec()),
            Self::Float32(v) => Ok(v.to_le_bytes().to_vec()),
            Self::Float64(v) => Ok(v.to_le_bytes().to_vec()),
            Self::Bool(v) => Ok(vec![if *v { 1 } else { 0 }]),
            Self::Bytes(b) => Ok(b.clone()),
            Self::Text(s) => Ok(s.as_bytes().to_vec()),
        }
    }

    pub(crate) fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "Null",
            Self::Int8(_) => "Int8",
            Self::Int16(_) => "Int16",
            Self::Int32(_) => "Int32",
            Self::Int64(_) => "Int64",
            Self::UInt8(_) => "UInt8",
            Self::UInt16(_) => "UInt16",
            Self::UInt32(_) => "UInt32",
            Self::UInt64(_) => "UInt64",
            Self::Float32(_) => "Float32",
            Self::Float64(_) => "Float64",
            Self::Bool(_) => "Bool",
            Self::Bytes(_) => "Bytes",
            Self::Text(_) => "Text",
        }
    }

    pub(crate) fn matches_scalar_type(&self, scalar_type: ScalarType) -> bool {
        matches!(
            (self, scalar_type),
            (Self::Int8(_), ScalarType::Int8)
                | (Self::Int16(_), ScalarType::Int16)
                | (Self::Int32(_), ScalarType::Int32)
                | (Self::Int64(_), ScalarType::Int64)
                | (Self::UInt8(_), ScalarType::UInt8)
                | (Self::UInt16(_), ScalarType::UInt16)
                | (Self::UInt32(_), ScalarType::UInt32)
                | (Self::UInt64(_), ScalarType::UInt64)
                | (Self::Float32(_), ScalarType::Float32)
                | (Self::Float64(_), ScalarType::Float64)
                | (Self::Bool(_), ScalarType::Bool)
        )
    }

    /// Decode from bytes given a scalar type.
    pub fn decode_scalar(scalar_type: ScalarType, bytes: &[u8]) -> AndromedaResult<Self> {
        match scalar_type {
            ScalarType::Int8 => {
                let byte = read_scalar_byte(bytes, "int8")?;
                Ok(Self::Int8(byte as i8))
            },
            ScalarType::Int16 => Ok(Self::Int16(i16::from_le_bytes(read_scalar_array(
                bytes, "int16",
            )?))),
            ScalarType::Int32 => Ok(Self::Int32(i32::from_le_bytes(read_scalar_array(
                bytes, "int32",
            )?))),
            ScalarType::Int64 => Ok(Self::Int64(i64::from_le_bytes(read_scalar_array(
                bytes, "int64",
            )?))),
            ScalarType::UInt8 => Ok(Self::UInt8(read_scalar_byte(bytes, "uint8")?)),
            ScalarType::UInt16 => Ok(Self::UInt16(u16::from_le_bytes(read_scalar_array(
                bytes, "uint16",
            )?))),
            ScalarType::UInt32 => Ok(Self::UInt32(u32::from_le_bytes(read_scalar_array(
                bytes, "uint32",
            )?))),
            ScalarType::UInt64 => Ok(Self::UInt64(u64::from_le_bytes(read_scalar_array(
                bytes, "uint64",
            )?))),
            ScalarType::Float32 => Ok(Self::Float32(f32::from_le_bytes(read_scalar_array(
                bytes, "float32",
            )?))),
            ScalarType::Float64 => Ok(Self::Float64(f64::from_le_bytes(read_scalar_array(
                bytes, "float64",
            )?))),
            ScalarType::Bool => Ok(Self::Bool(read_scalar_byte(bytes, "bool")? != 0)),
        }
    }
}
