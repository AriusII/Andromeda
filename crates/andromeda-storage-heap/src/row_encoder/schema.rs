use andromeda_error::AndromedaResult;

use super::error::encoder_error;
use super::format::{VAR_OFFSET_WIDTH_BYTES, bitmap_byte_len};

/// Scalar data types supported by Andromeda storage engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarType {
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float32,
    Float64,
    Bool,
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
    columns: Vec<ColumnDef>,
}

impl RowSchema {
    /// Create a new row schema from column definitions.
    pub fn new(columns: Vec<ColumnDef>) -> AndromedaResult<Self> {
        if columns.is_empty() {
            return Err(encoder_error("schema must have at least one column"));
        }

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

    pub fn columns(&self) -> &[ColumnDef] {
        &self.columns
    }

    /// Calculate fixed overhead for this schema (null bitmap + variable-length offsets).
    pub fn fixed_overhead_bytes(&self) -> usize {
        let null_bitmap_bytes = bitmap_byte_len(self.columns.len());
        let var_offset_bytes = self.variable_width_column_count() * VAR_OFFSET_WIDTH_BYTES;

        null_bitmap_bytes + var_offset_bytes
    }

    pub(crate) fn variable_width_column_count(&self) -> usize {
        self.columns
            .iter()
            .filter(|c| c.scalar_type.is_variable_width())
            .count()
    }

    pub(crate) fn fixed_width_payload_bytes(&self) -> usize {
        self.columns
            .iter()
            .filter(|c| !c.scalar_type.is_variable_width())
            .map(|c| c.scalar_type.fixed_byte_length())
            .sum()
    }
}
