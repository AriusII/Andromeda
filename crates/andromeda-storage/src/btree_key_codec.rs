//! B-Tree Key Codec - order-preserving key encoding and comparison.
//!
//! This module now acts as a compatibility facade over
//! `andromeda-storage-index`, which owns the durable byte contract.

use crate::heap_row_encoder::Datum;
use crate::heap_row_encoder::ScalarType;
use andromeda_core::AndromedaResult;
use andromeda_storage_index::{
    Key as IndexKey, KeyCodec as IndexKeyCodec, KeyDatum, KeyScalarType,
};

pub use andromeda_storage_index::KeyComparator;

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

#[derive(Debug, Clone)]
pub struct KeyCodec;

impl KeyCodec {
    pub fn encode_key(key: &Key) -> AndromedaResult<Vec<u8>> {
        IndexKeyCodec::encode_key(&to_index_key(key))
    }

    pub fn decode_key(bytes: &[u8]) -> AndromedaResult<Key> {
        IndexKeyCodec::decode_key(bytes).map(from_index_key)
    }

    pub fn encode_composite_key(cols: &[Datum]) -> AndromedaResult<Vec<u8>> {
        let cols: Vec<_> = cols.iter().map(to_index_datum).collect();
        IndexKeyCodec::encode_composite_key(&cols)
    }

    pub fn decode_composite(bytes: &[u8], schema: &[ScalarType]) -> AndromedaResult<Vec<Datum>> {
        let schema: Vec<_> = schema.iter().copied().map(to_index_scalar_type).collect();
        IndexKeyCodec::decode_composite(bytes, &schema)
            .map(|datums| datums.into_iter().map(from_index_datum).collect())
    }
}

fn to_index_key(key: &Key) -> IndexKey {
    match key {
        Key::Null => IndexKey::Null,
        Key::Int32(value) => IndexKey::Int32(*value),
        Key::Int64(value) => IndexKey::Int64(*value),
        Key::Text(value) => IndexKey::Text(value.clone()),
        Key::Bytes(value) => IndexKey::Bytes(value.clone()),
        Key::Composite(datums) => IndexKey::Composite(datums.iter().map(to_index_datum).collect()),
    }
}

fn from_index_key(key: IndexKey) -> Key {
    match key {
        IndexKey::Null => Key::Null,
        IndexKey::Int32(value) => Key::Int32(value),
        IndexKey::Int64(value) => Key::Int64(value),
        IndexKey::Text(value) => Key::Text(value),
        IndexKey::Bytes(value) => Key::Bytes(value),
        IndexKey::Composite(datums) => {
            Key::Composite(datums.into_iter().map(from_index_datum).collect())
        },
    }
}

fn to_index_datum(datum: &Datum) -> KeyDatum {
    match datum {
        Datum::Null => KeyDatum::Null,
        Datum::Int8(value) => KeyDatum::Int8(*value),
        Datum::Int16(value) => KeyDatum::Int16(*value),
        Datum::Int32(value) => KeyDatum::Int32(*value),
        Datum::Int64(value) => KeyDatum::Int64(*value),
        Datum::UInt8(value) => KeyDatum::UInt8(*value),
        Datum::UInt16(value) => KeyDatum::UInt16(*value),
        Datum::UInt32(value) => KeyDatum::UInt32(*value),
        Datum::UInt64(value) => KeyDatum::UInt64(*value),
        Datum::Float32(value) => KeyDatum::Float32(*value),
        Datum::Float64(value) => KeyDatum::Float64(*value),
        Datum::Bool(value) => KeyDatum::Bool(*value),
        Datum::Bytes(value) => KeyDatum::Bytes(value.clone()),
        Datum::Text(value) => KeyDatum::Text(value.clone()),
    }
}

fn from_index_datum(datum: KeyDatum) -> Datum {
    match datum {
        KeyDatum::Null => Datum::Null,
        KeyDatum::Int8(value) => Datum::Int8(value),
        KeyDatum::Int16(value) => Datum::Int16(value),
        KeyDatum::Int32(value) => Datum::Int32(value),
        KeyDatum::Int64(value) => Datum::Int64(value),
        KeyDatum::UInt8(value) => Datum::UInt8(value),
        KeyDatum::UInt16(value) => Datum::UInt16(value),
        KeyDatum::UInt32(value) => Datum::UInt32(value),
        KeyDatum::UInt64(value) => Datum::UInt64(value),
        KeyDatum::Float32(value) => Datum::Float32(value),
        KeyDatum::Float64(value) => Datum::Float64(value),
        KeyDatum::Bool(value) => Datum::Bool(value),
        KeyDatum::Bytes(value) => Datum::Bytes(value),
        KeyDatum::Text(value) => Datum::Text(value),
    }
}

fn to_index_scalar_type(scalar_type: ScalarType) -> KeyScalarType {
    match scalar_type {
        ScalarType::Int8 => KeyScalarType::Int8,
        ScalarType::Int16 => KeyScalarType::Int16,
        ScalarType::Int32 => KeyScalarType::Int32,
        ScalarType::Int64 => KeyScalarType::Int64,
        ScalarType::UInt8 => KeyScalarType::UInt8,
        ScalarType::UInt16 => KeyScalarType::UInt16,
        ScalarType::UInt32 => KeyScalarType::UInt32,
        ScalarType::UInt64 => KeyScalarType::UInt64,
        ScalarType::Float32 => KeyScalarType::Float32,
        ScalarType::Float64 => KeyScalarType::Float64,
        ScalarType::Bool => KeyScalarType::Bool,
    }
}
