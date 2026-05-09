use std::sync::Arc;

use andromeda_error::AndromedaResult;

use super::binary::read_array_at;
use super::datum::Datum;
use super::error::encoder_error;
use super::format::{BITS_PER_BITMAP_BYTE, VAR_OFFSET_WIDTH_BYTES, bitmap_byte_len};
use super::schema::RowSchema;

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
        let columns = self.schema.columns();
        if values.len() != columns.len() {
            return Err(encoder_error(format!(
                "value count mismatch: expected {}, got {}",
                columns.len(),
                values.len()
            )));
        }
        self.validate_values(values)?;

        let mut buffer = Vec::new();

        let null_bitmap = Self::encode_null_bitmap(values)?;
        buffer.extend_from_slice(&null_bitmap);

        let var_col_count = self.schema.variable_width_column_count();
        let fixed_width_bytes = self.schema.fixed_width_payload_bytes();

        // Offsets are absolute within the encoded row. Variable data starts
        // after the null bitmap, offset table, and fixed-width column area.
        let mut var_offset =
            null_bitmap.len() + (var_col_count * VAR_OFFSET_WIDTH_BYTES) + fixed_width_bytes;

        for (i, col) in columns.iter().enumerate() {
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

        for (i, col) in columns.iter().enumerate() {
            if !col.scalar_type.is_variable_width() {
                if !matches!(values[i], Datum::Null) {
                    buffer.extend_from_slice(&values[i].encode()?);
                } else {
                    // Fixed-width NULLs reserve their encoded column width with zeros.
                    buffer.resize(buffer.len() + col.scalar_type.fixed_byte_length(), 0);
                }
            }
        }

        for (i, col) in columns.iter().enumerate() {
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
        let columns = self.schema.columns();

        let null_bitmap = Self::decode_null_bitmap(bytes, columns.len(), &mut offset)?;

        let mut var_offsets = Vec::new();
        for col in columns {
            if col.scalar_type.is_variable_width() {
                let off = u32::from_le_bytes(read_array_at::<VAR_OFFSET_WIDTH_BYTES>(
                    bytes,
                    offset,
                    "truncated variable-length offset",
                )?) as usize;
                var_offsets.push(off);
                offset += VAR_OFFSET_WIDTH_BYTES;
            }
        }

        for (i, col) in columns.iter().enumerate() {
            if null_bitmap[i] {
                if !col.nullable {
                    return Err(encoder_error(format!(
                        "column {} ({}) is not nullable",
                        i, col.name
                    )));
                }
                if !col.scalar_type.is_variable_width() {
                    let col_bytes = col.scalar_type.fixed_byte_length();
                    if offset + col_bytes > bytes.len() {
                        return Err(encoder_error(format!(
                            "truncated fixed-length NULL storage at column {}",
                            i
                        )));
                    }
                    offset += col_bytes;
                }
                values.push(Datum::Null);
            } else if col.scalar_type.is_variable_width() {
                let var_idx = self
                    .schema
                    .columns()
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
    pub(crate) fn encode_null_bitmap(values: &[Datum]) -> AndromedaResult<Vec<u8>> {
        let bitmap_bytes = bitmap_byte_len(values.len());
        let mut bitmap = vec![0u8; bitmap_bytes];

        for (i, val) in values.iter().enumerate() {
            if matches!(val, Datum::Null) {
                bitmap[i / BITS_PER_BITMAP_BYTE] |= 1 << (i % BITS_PER_BITMAP_BYTE);
            }
        }

        Ok(bitmap)
    }

    fn decode_null_bitmap(
        bytes: &[u8],
        col_count: usize,
        offset: &mut usize,
    ) -> AndromedaResult<Vec<bool>> {
        let bitmap_bytes = bitmap_byte_len(col_count);

        if *offset + bitmap_bytes > bytes.len() {
            return Err(encoder_error("truncated null bitmap"));
        }

        let bitmap_slice = &bytes[*offset..*offset + bitmap_bytes];
        *offset += bitmap_bytes;

        Ok((0..col_count)
            .map(|i| {
                (bitmap_slice[i / BITS_PER_BITMAP_BYTE] & (1 << (i % BITS_PER_BITMAP_BYTE))) != 0
            })
            .collect())
    }

    fn validate_values(&self, values: &[Datum]) -> AndromedaResult<()> {
        for (i, (col, value)) in self.schema.columns().iter().zip(values).enumerate() {
            if matches!(value, Datum::Null) {
                if !col.nullable {
                    return Err(encoder_error(format!(
                        "column {} ({}) is not nullable",
                        i, col.name
                    )));
                }
                continue;
            }

            if !value.matches_scalar_type(col.scalar_type) {
                return Err(encoder_error(format!(
                    "column {} ({}) expected {:?}, got {}",
                    i,
                    col.name,
                    col.scalar_type,
                    value.type_name()
                )));
            }
        }
        Ok(())
    }
}
