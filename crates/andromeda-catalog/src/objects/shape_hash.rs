use andromeda_core::{
    AbsencePolicy, ColumnDescriptor, ContractHash, DecimalType, FloatMode, FloatType, ScalarType,
    TextEncoding, TimestampType, TypeDescriptor,
};

use crate::{contracts::ProcedureContract, digest::Sha256};

use super::{
    CatalogObjectRef, EnumDefinition, ObjectKind, StructuredObjectDefinition, TableDefinition,
};

pub(super) fn table_shape_hash(definition: &TableDefinition) -> ContractHash {
    let mut sink = ObjectShapeHashSink::new();
    sink.str("andromeda.catalog.table-shape.v2.sha256");
    sink.object_ref(&definition.object);
    sink.columns(&definition.columns);
    sink.finish()
}

pub(super) fn structured_object_shape_hash(
    definition: &StructuredObjectDefinition,
) -> ContractHash {
    let mut sink = ObjectShapeHashSink::new();
    sink.str("andromeda.catalog.structured-object-shape.v2.sha256");
    sink.object_ref(&definition.object);
    sink.columns(&definition.fields);
    sink.u64(definition.unique_by.len() as u64);
    for field in &definition.unique_by {
        sink.str(field);
    }
    sink.finish()
}

pub(super) fn enum_shape_hash(definition: &EnumDefinition) -> ContractHash {
    let mut sink = ObjectShapeHashSink::new();
    sink.str("andromeda.catalog.enum-shape.v2.sha256");
    sink.object_ref(&definition.object);
    sink.u64(definition.variants.len() as u64);
    for variant in &definition.variants {
        sink.str(&variant.name);
        sink.raw_bytes(&variant.value.to_le_bytes());
    }
    sink.finish()
}

pub(super) fn procedure_shape_hash(definition: &ProcedureContract) -> ContractHash {
    let mut sink = ObjectShapeHashSink::new();
    sink.str("andromeda.catalog.procedure-definition-shape.v1.sha256");
    sink.object_ref(&definition.object);
    sink.u64(definition.procedure_id.get());
    sink.raw_bytes(&definition.contract_hash.as_bytes());
    sink.u64(definition.stats_version.get());
    sink.raw_bytes(&definition.policy_version().as_bytes());
    sink.finish()
}

struct ObjectShapeHashSink {
    hasher: Sha256,
}

impl ObjectShapeHashSink {
    fn new() -> Self {
        Self {
            hasher: Sha256::new(),
        }
    }

    fn finish(self) -> ContractHash {
        ContractHash::new(self.hasher.finalize())
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.u64(bytes.len() as u64);
        self.raw_bytes(bytes);
    }

    fn raw_bytes(&mut self, bytes: &[u8]) {
        self.hasher.update(bytes);
    }

    fn u8(&mut self, value: u8) {
        self.hasher.update(&[value]);
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

    fn object_ref(&mut self, object: &CatalogObjectRef) {
        self.u64(object.object_id.get());
        self.u64(object.catalog_version.get());
        self.u8(match object.kind {
            ObjectKind::Database => 0,
            ObjectKind::Namespace => 1,
            ObjectKind::Table => 2,
            ObjectKind::Map => 3,
            ObjectKind::Enum => 4,
            ObjectKind::StructuredObject => 5,
            ObjectKind::Procedure => 6,
        });
        self.u64(object.name.parts().len() as u64);
        for part in object.name.parts() {
            self.str(part);
        }
    }

    fn columns(&mut self, columns: &[ColumnDescriptor]) {
        self.u64(columns.len() as u64);
        for column in columns {
            self.str(&column.name);
            self.type_descriptor(&column.data_type);
            self.u32(column.ordinal);
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
            }
            ScalarType::Float(float) => {
                self.u8(11);
                self.float_type(*float);
            }
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
                    }
                    None => self.bool(false),
                }
            }
            ScalarType::Timestamp(timestamp) => {
                self.u8(14);
                self.u8(match timestamp {
                    TimestampType::Transaction => 0,
                    TimestampType::Invocation => 1,
                    TimestampType::MonotonicEpoch => 2,
                });
            }
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
            }
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
            }
        }
    }
}
