//! B-Tree Key Codec - order-preserving key encoding and comparison.
//!
//! This module preserves the public KeyV1 API while splitting durable-byte
//! responsibilities across codec, primitive encoding, comparison, validation,
//! and golden-test helper modules.

use crate::heap_row_encoder::Datum;

mod codec;
mod comparator;
mod primitive;
mod validation;

pub use codec::KeyCodec;
pub use comparator::KeyComparator;

/// A key value that can be encoded/decoded.
#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    Null,
    Int32(i32),
    Int64(i64),
    Text(String),
    Bytes(Vec<u8>),
    /// Composite key: sequence of Datums.
    Composite(Vec<Datum>),
}
