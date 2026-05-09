use std::collections::BTreeSet;

use andromeda_digest::Sha256;
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{
    AbsencePolicy, ColumnDescriptor, ContractHash, DecimalType, FloatMode, FloatType, ScalarType,
    TextEncoding, TimestampType, TypeDescriptor,
};

const STRUCTURED_OBJECT_SHAPE_HASH_TAG: &str = "andromeda.structured-object.shape.v1.sha256";

pub(super) fn validate_structured_object_shape(
    fields: &[ColumnDescriptor],
    unique_by: &[String],
) -> AndromedaResult<()> {
    if fields.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "column list must not be empty",
        ));
    }

    let mut field_names = BTreeSet::new();
    for (expected_ordinal, field) in fields.iter().enumerate() {
        field.validate()?;
        if !field_names.insert(field.name.as_str()) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "column names must be unique",
            ));
        }
        if field.ordinal != expected_ordinal as u32 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "column ordinals must be dense and zero-based",
            ));
        }
    }

    for unique_field in unique_by {
        if !fields.iter().any(|field| &field.name == unique_field) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "structured object unique key must reference an existing field",
            ));
        }
    }

    Ok(())
}

pub(super) fn encode_structured_object_shape_material(
    fields: &[ColumnDescriptor],
    unique_by: &[String],
) -> Vec<u8> {
    let mut sink = StructuredObjectShapeMaterialSink::new();
    sink.columns(fields);
    sink.unique_by(unique_by);
    sink.finish_material()
}

pub(super) fn encode_column_descriptors_shape_material(columns: &[ColumnDescriptor]) -> Vec<u8> {
    let mut sink = StructuredObjectShapeMaterialSink::new();
    sink.columns(columns);
    sink.finish_material()
}

pub(super) fn compute_structured_object_shape_hash(
    fields: &[ColumnDescriptor],
    unique_by: &[String],
) -> ContractHash {
    let mut sink = StructuredObjectShapeMaterialSink::new();
    sink.str(STRUCTURED_OBJECT_SHAPE_HASH_TAG);
    sink.columns(fields);
    sink.unique_by(unique_by);
    sink.finish_hash()
}

struct StructuredObjectShapeMaterialSink {
    material: Vec<u8>,
}

impl StructuredObjectShapeMaterialSink {
    fn new() -> Self {
        Self {
            material: Vec::with_capacity(256),
        }
    }

    fn finish_material(self) -> Vec<u8> {
        self.material
    }

    fn finish_hash(self) -> ContractHash {
        let mut hasher = Sha256::new();
        hasher.update(&self.material);
        ContractHash::new(hasher.finalize())
    }

    fn raw_bytes(&mut self, bytes: &[u8]) {
        self.material.extend_from_slice(bytes);
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.u64(bytes.len() as u64);
        self.raw_bytes(bytes);
    }

    fn u8(&mut self, value: u8) {
        self.material.push(value);
    }

    fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    fn u32(&mut self, value: u32) {
        self.raw_bytes(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.raw_bytes(&value.to_le_bytes());
    }

    fn str(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn columns(&mut self, columns: &[ColumnDescriptor]) {
        self.u64(columns.len() as u64);
        for column in columns {
            self.str(&column.name);
            self.type_descriptor(&column.data_type);
            self.u32(column.ordinal);
        }
    }

    fn unique_by(&mut self, unique_by: &[String]) {
        self.u64(unique_by.len() as u64);
        for key in unique_by {
            self.str(key);
        }
    }

    fn type_descriptor(&mut self, descriptor: &TypeDescriptor) {
        self.scalar_type(&descriptor.scalar);
        self.u8(match descriptor.absence {
            AbsencePolicy::Required => 0,
            AbsencePolicy::ExplicitOptional => 1,
        });
    }

    fn scalar_type(&mut self, scalar: &ScalarType) {
        match scalar {
            ScalarType::I8 => self.u8(0),
            ScalarType::I16 => self.u8(1),
            ScalarType::I32 => self.u8(2),
            ScalarType::I64 => self.u8(3),
            ScalarType::I128 => self.u8(4),
            ScalarType::U8 => self.u8(5),
            ScalarType::U16 => self.u8(6),
            ScalarType::U32 => self.u8(7),
            ScalarType::U64 => self.u8(8),
            ScalarType::U128 => self.u8(9),
            ScalarType::Decimal(decimal) => {
                self.u8(10);
                self.decimal_type(*decimal);
            },
            ScalarType::Float(float) => {
                self.u8(11);
                self.float_type(*float);
            },
            ScalarType::Bool => self.u8(12),
            ScalarType::Text(text) => {
                self.u8(13);
                self.u8(match text.encoding {
                    TextEncoding::Utf8 => 0,
                    TextEncoding::Utf16 => 1,
                    TextEncoding::Unicode => 2,
                });
                self.u32(text.max_length.unwrap_or(0));
                self.bool(text.max_length.is_some());
                match &text.collation {
                    Some(collation) => {
                        self.bool(true);
                        self.str(collation);
                    },
                    None => self.bool(false),
                }
            },
            ScalarType::Timestamp(timestamp) => {
                self.u8(14);
                self.u8(match timestamp {
                    TimestampType::Transaction => 0,
                    TimestampType::Invocation => 1,
                    TimestampType::MonotonicEpoch => 2,
                });
            },
        }
    }

    fn decimal_type(&mut self, decimal: DecimalType) {
        match decimal {
            DecimalType::Min => self.u8(0),
            DecimalType::Mid => self.u8(1),
            DecimalType::Max => self.u8(2),
            DecimalType::Custom { precision, scale } => {
                self.u8(3);
                self.u8(precision);
                self.u8(scale);
            },
        }
    }

    fn float_type(&mut self, float: FloatType) {
        match float {
            FloatType::Min => self.u8(0),
            FloatType::Mid => self.u8(1),
            FloatType::Max => self.u8(2),
            FloatType::Custom { bits, mode } => {
                self.u8(3);
                self.raw_bytes(&bits.to_le_bytes());
                self.u8(match mode {
                    FloatMode::Approximate => 0,
                    FloatMode::DeterministicAnalytics => 1,
                });
            },
        }
    }
}
