//! Canonical explicit encoding for contract type descriptors.
//!
//! This module is used by both canonical contract hashing and manifest hashing
//! to avoid Debug/struct serialization fallbacks.

use andromeda_types::{
    AbsencePolicy, DecimalType, FloatMode, FloatType, ScalarType, TextEncoding, TimestampType,
    TypeDescriptor,
};

pub(crate) fn canonical_type_descriptor_bytes(descriptor: &TypeDescriptor) -> Vec<u8> {
    let mut sink = Vec::with_capacity(32);
    scalar_type_bytes(&mut sink, &descriptor.scalar);
    sink.push(match descriptor.absence {
        AbsencePolicy::Required => 0,
        AbsencePolicy::ExplicitOptional => 1,
    });
    sink
}

fn scalar_type_bytes(sink: &mut Vec<u8>, scalar: &ScalarType) {
    match scalar {
        ScalarType::I8 => sink.push(0),
        ScalarType::I16 => sink.push(1),
        ScalarType::I32 => sink.push(2),
        ScalarType::I64 => sink.push(3),
        ScalarType::I128 => sink.push(4),
        ScalarType::U8 => sink.push(5),
        ScalarType::U16 => sink.push(6),
        ScalarType::U32 => sink.push(7),
        ScalarType::U64 => sink.push(8),
        ScalarType::U128 => sink.push(9),
        ScalarType::Decimal(decimal) => {
            sink.push(10);
            decimal_type_bytes(sink, *decimal);
        },
        ScalarType::Float(float) => {
            sink.push(11);
            float_type_bytes(sink, *float);
        },
        ScalarType::Bool => sink.push(12),
        ScalarType::Text(text) => {
            sink.push(13);
            sink.push(match text.encoding {
                TextEncoding::Utf8 => 0,
                TextEncoding::Utf16 => 1,
                TextEncoding::Unicode => 2,
            });
            sink.extend_from_slice(&text.max_length.unwrap_or(0).to_le_bytes());
            sink.push(u8::from(text.max_length.is_some()));
            match &text.collation {
                Some(collation) => {
                    sink.push(1);
                    len_prefixed_bytes(sink, collation.as_bytes());
                },
                None => sink.push(0),
            }
        },
        ScalarType::Timestamp(timestamp) => {
            sink.push(14);
            sink.push(match timestamp {
                TimestampType::Transaction => 0,
                TimestampType::Invocation => 1,
                TimestampType::MonotonicEpoch => 2,
            });
        },
    }
}

fn decimal_type_bytes(sink: &mut Vec<u8>, decimal: DecimalType) {
    match decimal {
        DecimalType::Min => sink.push(0),
        DecimalType::Mid => sink.push(1),
        DecimalType::Max => sink.push(2),
        DecimalType::Custom { precision, scale } => {
            sink.push(3);
            sink.push(precision);
            sink.push(scale);
        },
    }
}

fn float_type_bytes(sink: &mut Vec<u8>, float: FloatType) {
    match float {
        FloatType::Min => sink.push(0),
        FloatType::Mid => sink.push(1),
        FloatType::Max => sink.push(2),
        FloatType::Custom { bits, mode } => {
            sink.push(3);
            sink.extend_from_slice(&bits.to_le_bytes());
            sink.push(match mode {
                FloatMode::Approximate => 0,
                FloatMode::DeterministicAnalytics => 1,
            });
        },
    }
}

fn len_prefixed_bytes(sink: &mut Vec<u8>, bytes: &[u8]) {
    sink.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    sink.extend_from_slice(bytes);
}
