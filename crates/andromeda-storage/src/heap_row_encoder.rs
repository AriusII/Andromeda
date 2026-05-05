//! Row encoding and decoding — Converts structured tuples to/from binary format.
//!
//! This module implements serialization of row data to compact binary format suitable
//! for heap page storage. Each row is encoded with:
//!
//! 1. **Null Bitmap**: One bit per column (1 = null, 0 = not null)
//! 2. **Fixed-length Columns**: Stored sequentially
//! 3. **Variable-length Columns**: Stored with offset/length metadata
//!
//! All numeric values use little-endian byte order.

use std::sync::Arc;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// Scalar data types supported by Andromeda storage engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarType {
    // Integers
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    // Floating point
    Float32,
    Float64,
    // Boolean
    Bool,
    // Variable-length types must use Datum::Bytes or Datum::Text
}

impl ScalarType {
    /// Get fixed byte length for fixed-width types, or 0 for variable-width.
    pub fn fixed_byte_length(self) -> usize {
        match self {
            Self::Int8 => 1,
            Self::Int16 => 2,
            Self::Int32 => 4,
            Self::Int64 => 8,
            Self::UInt8 => 1,
            Self::UInt16 => 2,
            Self::UInt32 => 4,
            Self::UInt64 => 8,
            Self::Float32 => 4,
            Self::Float64 => 8,
            Self::Bool => 1,
        }
    }

    /// Check if this type is variable-width.
    pub fn is_variable_width(self) -> bool {
        self.fixed_byte_length() == 0
    }
}

/// A single column in a row schema.
#[derive(Debug, Clone)]
pub struct ColumnDef {
    pub name: String,
    pub ordinal: u16,
    pub scalar_type: ScalarType,
    pub nullable: bool,
}

/// Row schema — describes the structure of encoded rows.
#[derive(Debug, Clone)]
pub struct RowSchema {
    pub columns: Vec<ColumnDef>,
}

impl RowSchema {
    /// Create a new row schema from column definitions.
    pub fn new(columns: Vec<ColumnDef>) -> AndromedaResult<Self> {
        if columns.is_empty() {
            return Err(encoder_error("schema must have at least one column"));
        }

        // Validate ordinals
        for (i, col) in columns.iter().enumerate() {
            if col.ordinal as usize != i {
                return Err(encoder_error(format!(
                    "column ordinals must be dense and zero-based, found: {}",
                    col.ordinal
                )));
            }
        }

        Ok(Self { columns })
    }

    /// Calculate fixed overhead for this schema (null bitmap + variable-length offsets).
    pub fn fixed_overhead_bytes(&self) -> usize {
        let null_bitmap_bytes = (self.columns.len() + 7) / 8;
        let var_offset_bytes = self
            .columns
            .iter()
            .filter(|c| c.scalar_type.is_variable_width())
            .count()
            * 4;

        null_bitmap_bytes + var_offset_bytes
    }
}

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
            }
            Self::Text(s) => {
                let bytes = s.as_bytes();
                if bytes.len() > u32::MAX as usize {
                    return Err(encoder_error("text too large"));
                }
                Ok(bytes.len())
            }
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

    /// Decode from bytes given a scalar type.
    pub fn decode_scalar(scalar_type: ScalarType, bytes: &[u8]) -> AndromedaResult<Self> {
        match scalar_type {
            ScalarType::Int8 => {
                if bytes.len() < 1 {
                    return Err(encoder_error("insufficient bytes for int8"));
                }
                Ok(Self::Int8(bytes[0] as i8))
            }
            ScalarType::Int16 => {
                if bytes.len() < 2 {
                    return Err(encoder_error("insufficient bytes for int16"));
                }
                Ok(Self::Int16(i16::from_le_bytes([bytes[0], bytes[1]])))
            }
            ScalarType::Int32 => {
                if bytes.len() < 4 {
                    return Err(encoder_error("insufficient bytes for int32"));
                }
                Ok(Self::Int32(i32::from_le_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3],
                ])))
            }
            ScalarType::Int64 => {
                if bytes.len() < 8 {
                    return Err(encoder_error("insufficient bytes for int64"));
                }
                Ok(Self::Int64(i64::from_le_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
                ])))
            }
            ScalarType::UInt8 => {
                if bytes.len() < 1 {
                    return Err(encoder_error("insufficient bytes for uint8"));
                }
                Ok(Self::UInt8(bytes[0]))
            }
            ScalarType::UInt16 => {
                if bytes.len() < 2 {
                    return Err(encoder_error("insufficient bytes for uint16"));
                }
                Ok(Self::UInt16(u16::from_le_bytes([bytes[0], bytes[1]])))
            }
            ScalarType::UInt32 => {
                if bytes.len() < 4 {
                    return Err(encoder_error("insufficient bytes for uint32"));
                }
                Ok(Self::UInt32(u32::from_le_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3],
                ])))
            }
            ScalarType::UInt64 => {
                if bytes.len() < 8 {
                    return Err(encoder_error("insufficient bytes for uint64"));
                }
                Ok(Self::UInt64(u64::from_le_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
                ])))
            }
            ScalarType::Float32 => {
                if bytes.len() < 4 {
                    return Err(encoder_error("insufficient bytes for float32"));
                }
                Ok(Self::Float32(f32::from_le_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3],
                ])))
            }
            ScalarType::Float64 => {
                if bytes.len() < 8 {
                    return Err(encoder_error("insufficient bytes for float64"));
                }
                Ok(Self::Float64(f64::from_le_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
                ])))
            }
            ScalarType::Bool => {
                if bytes.len() < 1 {
                    return Err(encoder_error("insufficient bytes for bool"));
                }
                Ok(Self::Bool(bytes[0] != 0))
            }
        }
    }
}

/// Row encoder — serializes and deserializes rows according to a schema.
#[derive(Debug, Clone)]
pub struct RowEncoder {
    schema: Arc<RowSchema>,
}

impl RowEncoder {
    /// Create a new encoder for the given schema.
    pub fn new(schema: Arc<RowSchema>) -> Self {
        Self { schema }
    }

    /// Encode a row (list of datums) to binary format.
    pub fn encode(&self, values: &[Datum]) -> AndromedaResult<Vec<u8>> {
        if values.len() != self.schema.columns.len() {
            return Err(encoder_error(format!(
                "value count mismatch: expected {}, got {}",
                self.schema.columns.len(),
                values.len()
            )));
        }

        let mut buffer = Vec::new();

        // Encode null bitmap
        let null_bitmap = Self::encode_null_bitmap(values)?;
        buffer.extend_from_slice(&null_bitmap);

        // Count variable-length columns
        let var_col_count = self
            .schema
            .columns
            .iter()
            .filter(|c| c.scalar_type.is_variable_width())
            .count();

        let fixed_width_bytes: usize = self
            .schema
            .columns
            .iter()
            .filter(|c| !c.scalar_type.is_variable_width())
            .map(|c| c.scalar_type.fixed_byte_length())
            .sum();

        // Write variable-length offsets (4 bytes each). Offsets are absolute within the
        // encoded row, so variable data starts after the null bitmap, offset table, and
        // fixed-width column area.
        let mut var_offset = null_bitmap.len() + (var_col_count * 4) + fixed_width_bytes;

        for (i, col) in self.schema.columns.iter().enumerate() {
            if col.scalar_type.is_variable_width() {
                if matches!(values[i], Datum::Null) {
                    buffer.extend_from_slice(&0u32.to_le_bytes());
                } else {
                    let len = values[i].byte_length()?;
                    buffer.extend_from_slice(&(var_offset as u32).to_le_bytes());
                    var_offset += len;
                }
            }
        }

        // Write fixed-length columns
        for (i, col) in self.schema.columns.iter().enumerate() {
            if !col.scalar_type.is_variable_width() {
                if !matches!(values[i], Datum::Null) {
                    buffer.extend_from_slice(&values[i].encode()?);
                } else {
                    // For fixed-width nulls, write zeros as placeholder
                    buffer.resize(buffer.len() + col.scalar_type.fixed_byte_length(), 0);
                }
            }
        }

        // Write variable-length data
        for (i, col) in self.schema.columns.iter().enumerate() {
            if col.scalar_type.is_variable_width() && !matches!(values[i], Datum::Null) {
                buffer.extend_from_slice(&values[i].encode()?);
            }
        }

        Ok(buffer)
    }

    /// Decode a row from binary format.
    pub fn decode(&self, bytes: &[u8]) -> AndromedaResult<Vec<Datum>> {
        let mut offset = 0;
        let mut values = Vec::new();

        // Read null bitmap
        let null_bitmap = Self::decode_null_bitmap(bytes, self.schema.columns.len(), &mut offset)?;

        // Read variable-length offsets
        let mut var_offsets = Vec::new();
        for col in self.schema.columns.iter() {
            if col.scalar_type.is_variable_width() {
                if offset + 4 > bytes.len() {
                    return Err(encoder_error("truncated variable-length offset"));
                }
                let off = u32::from_le_bytes([
                    bytes[offset],
                    bytes[offset + 1],
                    bytes[offset + 2],
                    bytes[offset + 3],
                ]) as usize;
                var_offsets.push(off);
                offset += 4;
            }
        }

        // Decode all columns
        for (i, col) in self.schema.columns.iter().enumerate() {
            if null_bitmap[i] {
                if !col.scalar_type.is_variable_width() {
                    let col_bytes = col.scalar_type.fixed_byte_length();
                    if offset + col_bytes > bytes.len() {
                        return Err(encoder_error(format!(
                            "truncated fixed-length null placeholder at column {}",
                            i
                        )));
                    }
                    offset += col_bytes;
                }
                values.push(Datum::Null);
            } else if col.scalar_type.is_variable_width() {
                let var_idx = self
                    .schema
                    .columns
                    .iter()
                    .take(i)
                    .filter(|c| c.scalar_type.is_variable_width())
                    .count();
                let start = var_offsets[var_idx];

                let end = var_offsets
                    .iter()
                    .skip(var_idx + 1)
                    .copied()
                    .find(|off| *off != 0)
                    .unwrap_or(bytes.len());

                if start > end || end > bytes.len() {
                    return Err(encoder_error("invalid variable-length offset"));
                }

                // Heuristic: if column name contains "text" (future: check column type)
                // For now, store as bytes
                values.push(Datum::Bytes(bytes[start..end].to_vec()));
            } else {
                let col_bytes = col.scalar_type.fixed_byte_length();
                if offset + col_bytes > bytes.len() {
                    return Err(encoder_error(format!(
                        "truncated fixed-length value at column {}",
                        i
                    )));
                }

                let datum =
                    Datum::decode_scalar(col.scalar_type, &bytes[offset..offset + col_bytes])?;
                values.push(datum);
                offset += col_bytes;
            }
        }

        Ok(values)
    }

    /// Encode null bitmap (1 bit per column).
    fn encode_null_bitmap(values: &[Datum]) -> AndromedaResult<Vec<u8>> {
        let bitmap_bytes = (values.len() + 7) / 8;
        let mut bitmap = vec![0u8; bitmap_bytes];

        for (i, val) in values.iter().enumerate() {
            if matches!(val, Datum::Null) {
                bitmap[i / 8] |= 1 << (i % 8);
            }
        }

        Ok(bitmap)
    }

    /// Decode null bitmap.
    fn decode_null_bitmap(
        bytes: &[u8],
        col_count: usize,
        offset: &mut usize,
    ) -> AndromedaResult<Vec<bool>> {
        let bitmap_bytes = (col_count + 7) / 8;

        if *offset + bitmap_bytes > bytes.len() {
            return Err(encoder_error("truncated null bitmap"));
        }

        let bitmap_slice = &bytes[*offset..*offset + bitmap_bytes];
        *offset += bitmap_bytes;

        Ok((0..col_count)
            .map(|i| (bitmap_slice[i / 8] & (1 << (i % 8))) != 0)
            .collect())
    }
}

/// Helper to create encoder errors.
fn encoder_error(msg: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_schema() -> Arc<RowSchema> {
        Arc::new(
            RowSchema::new(vec![
                ColumnDef {
                    name: "id".to_string(),
                    ordinal: 0,
                    scalar_type: ScalarType::Int64,
                    nullable: false,
                },
                ColumnDef {
                    name: "name".to_string(),
                    ordinal: 1,
                    scalar_type: ScalarType::UInt32, // Placeholder for variable-length
                    nullable: true,
                },
                ColumnDef {
                    name: "active".to_string(),
                    ordinal: 2,
                    scalar_type: ScalarType::Bool,
                    nullable: true,
                },
            ])
            .expect("schema creation failed"),
        )
    }

    #[test]
    fn test_null_bitmap_encoding() {
        let values = vec![Datum::Int64(42), Datum::Null, Datum::Bool(true)];
        let bitmap = RowEncoder::encode_null_bitmap(&values).expect("encode failed");

        // Bit 1 should be set
        assert_eq!(bitmap[0], 0x02);
    }

    #[test]
    fn test_scalar_type_byte_lengths() {
        assert_eq!(ScalarType::Int8.fixed_byte_length(), 1);
        assert_eq!(ScalarType::Int16.fixed_byte_length(), 2);
        assert_eq!(ScalarType::Int32.fixed_byte_length(), 4);
        assert_eq!(ScalarType::Int64.fixed_byte_length(), 8);
        assert_eq!(ScalarType::Float32.fixed_byte_length(), 4);
        assert_eq!(ScalarType::Float64.fixed_byte_length(), 8);
        assert_eq!(ScalarType::Bool.fixed_byte_length(), 1);
    }

    #[test]
    fn test_datum_byte_lengths() {
        assert_eq!(Datum::Int64(42).byte_length().unwrap(), 8);
        assert_eq!(Datum::Bool(true).byte_length().unwrap(), 1);
        assert_eq!(Datum::Bytes(vec![1, 2, 3]).byte_length().unwrap(), 3);
    }

    #[test]
    fn test_datum_roundtrip_int64() {
        let original = Datum::Int64(12345);
        let encoded = original.encode().expect("encode failed");
        let decoded = Datum::decode_scalar(ScalarType::Int64, &encoded).expect("decode failed");
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_datum_roundtrip_bool() {
        let original = Datum::Bool(true);
        let encoded = original.encode().expect("encode failed");
        let decoded = Datum::decode_scalar(ScalarType::Bool, &encoded).expect("decode failed");
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_row_encoder_roundtrip() {
        let schema = create_test_schema();
        let encoder = RowEncoder::new(schema.clone());

        let values = vec![Datum::Int64(42), Datum::Null, Datum::Bool(true)];

        let encoded = encoder.encode(&values).expect("encode failed");
        let decoded = encoder.decode(&encoded).expect("decode failed");

        // Note: Bytes(vec![]) for null text
        assert_eq!(decoded[0], Datum::Int64(42));
        assert_eq!(decoded[1], Datum::Null);
        assert_eq!(decoded[2], Datum::Bool(true));
    }

    #[test]
    fn test_row_encoder_mismatch() {
        let schema = create_test_schema();
        let encoder = RowEncoder::new(schema.clone());

        let values = vec![Datum::Int64(42)]; // Wrong count
        let result = encoder.encode(&values);

        assert!(result.is_err());
    }
}
