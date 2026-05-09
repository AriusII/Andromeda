//! Index-owned order-preserving B-Tree key codec.
//!
//! The storage crate keeps a compatibility facade that converts its historical
//! `Datum` and `ScalarType` contracts into these index-owned types.

mod codec;
mod primitive;
mod validation;

/// Durable B-Tree key codec owner.
#[derive(Debug, Clone)]
pub struct KeyCodec;

/// Datum variants accepted by the durable B-Tree key codec.
#[derive(Debug, Clone, PartialEq)]
pub enum KeyDatum {
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

/// Scalar schema types accepted when validating composite keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyScalarType {
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

/// Public key type preserved by the storage compatibility facade.
#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    Null,
    Int32(i32),
    Int64(i64),
    Text(String),
    Bytes(Vec<u8>),
    Composite(Vec<KeyDatum>),
}

impl KeyDatum {
    pub(crate) fn matches_scalar_type(&self, scalar_type: KeyScalarType) -> bool {
        matches!(
            (self, scalar_type),
            (Self::Int8(_), KeyScalarType::Int8)
                | (Self::Int16(_), KeyScalarType::Int16)
                | (Self::Int32(_), KeyScalarType::Int32)
                | (Self::Int64(_), KeyScalarType::Int64)
                | (Self::UInt8(_), KeyScalarType::UInt8)
                | (Self::UInt16(_), KeyScalarType::UInt16)
                | (Self::UInt32(_), KeyScalarType::UInt32)
                | (Self::UInt64(_), KeyScalarType::UInt64)
                | (Self::Float32(_), KeyScalarType::Float32)
                | (Self::Float64(_), KeyScalarType::Float64)
                | (Self::Bool(_), KeyScalarType::Bool)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Key, KeyCodec, KeyDatum, KeyScalarType};
    use andromeda_error::AndromedaResult;

    #[test]
    fn codec_round_trips_composite_keys() -> AndromedaResult<()> {
        let key = Key::Composite(vec![
            KeyDatum::Int32(10),
            KeyDatum::Text("hello".to_string()),
            KeyDatum::Bool(true),
        ]);

        let encoded = KeyCodec::encode_key(&key)?;
        let decoded = KeyCodec::decode_key(&encoded)?;

        assert_eq!(decoded, key);
        Ok(())
    }

    #[test]
    fn codec_validates_composite_schema_arity_and_types() -> AndromedaResult<()> {
        let encoded = KeyCodec::encode_composite_key(&[
            KeyDatum::Int32(10),
            KeyDatum::Int64(20),
            KeyDatum::Bool(false),
        ])?;

        let decoded = KeyCodec::decode_composite(
            &encoded,
            &[
                KeyScalarType::Int32,
                KeyScalarType::Int64,
                KeyScalarType::Bool,
            ],
        )?;
        assert_eq!(
            decoded,
            vec![
                KeyDatum::Int32(10),
                KeyDatum::Int64(20),
                KeyDatum::Bool(false),
            ]
        );

        assert!(
            KeyCodec::decode_composite(&encoded, &[KeyScalarType::Int32, KeyScalarType::Int64])
                .is_err()
        );

        Ok(())
    }
}
