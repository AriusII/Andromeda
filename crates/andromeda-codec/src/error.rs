use std::{error::Error, fmt};

pub type CodecResult<T> = Result<T, CodecError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodecBoundsError {
    pub offset: usize,
    pub needed: usize,
    pub available: usize,
}

impl CodecBoundsError {
    pub const fn new(offset: usize, needed: usize, available: usize) -> Self {
        Self {
            offset,
            needed,
            available,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecError {
    ReadOutOfBounds(CodecBoundsError),
    WriteOutOfBounds(CodecBoundsError),
    OffsetOverflow { offset: usize, needed: usize },
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadOutOfBounds(bounds) => write!(
                f,
                "little-endian read at offset {} needs {} bytes but only {} remain",
                bounds.offset, bounds.needed, bounds.available
            ),
            Self::WriteOutOfBounds(bounds) => write!(
                f,
                "little-endian write at offset {} needs {} bytes but only {} remain",
                bounds.offset, bounds.needed, bounds.available
            ),
            Self::OffsetOverflow { offset, needed } => write!(
                f,
                "little-endian cursor offset {offset} overflows when advanced by {needed} bytes"
            ),
        }
    }
}

impl Error for CodecError {}
