use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
    ColumnDescriptor, ContractHash, DecimalType, FloatMode, FloatType, ScalarType, TextEncoding,
    TimestampType,
};
use std::collections::BTreeSet;

use crate::{contracts::ProcedureContract, names::QualifiedName};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Database,
    Namespace,
    Table,
    Map,
    Enum,
    StructuredObject,
    Procedure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogObjectRef {
    pub object_id: CatalogObjectId,
    pub name: QualifiedName,
    pub kind: ObjectKind,
    pub catalog_version: CatalogVersion,
}

impl CatalogObjectRef {
    pub fn validate_for_definition(&self, expected_kind: ObjectKind) -> AndromedaResult<()> {
        if self.object_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog object id must not be zero",
            ));
        }

        if self.catalog_version.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog object version must not be zero",
            ));
        }

        if self.kind != expected_kind {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog object kind must match its definition",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogBindingKind {
    ReadsTable,
    WritesTable,
    UsesStructuredInput,
    EmitsStructuredObject,
}

impl CatalogBindingKind {
    pub fn dependency_kind(self) -> ObjectKind {
        match self {
            Self::ReadsTable | Self::WritesTable => ObjectKind::Table,
            Self::UsesStructuredInput | Self::EmitsStructuredObject => ObjectKind::StructuredObject,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogObjectBinding {
    pub dependent: CatalogObjectRef,
    pub dependency: CatalogObjectRef,
    pub kind: CatalogBindingKind,
}

impl CatalogObjectBinding {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.dependent
            .validate_for_definition(ObjectKind::Procedure)?;
        self.dependency
            .validate_for_definition(self.kind.dependency_kind())?;

        if self.dependent.object_id == self.dependency.object_id {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog binding dependent and dependency object ids must differ",
            ));
        }

        if self.dependent.name == self.dependency.name {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog binding dependent and dependency names must differ",
            ));
        }

        if self.dependent.catalog_version != self.dependency.catalog_version {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog binding evidence requires exact catalog version match",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDefinition {
    pub object: CatalogObjectRef,
    pub columns: Vec<ColumnDescriptor>,
}

impl TableDefinition {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.object.validate_for_definition(ObjectKind::Table)?;
        validate_columns(&self.columns)
    }

    pub fn shape_hash(&self) -> ContractHash {
        let mut sink = ObjectShapeHashSink::new();
        sink.str("andromeda.catalog.table-shape.v1");
        sink.object_ref(&self.object);
        sink.columns(&self.columns);
        sink.finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredObjectDefinition {
    pub object: CatalogObjectRef,
    pub fields: Vec<ColumnDescriptor>,
    pub unique_by: Vec<String>,
}

impl StructuredObjectDefinition {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.object
            .validate_for_definition(ObjectKind::StructuredObject)?;
        validate_columns(&self.fields)?;

        for unique_field in &self.unique_by {
            if !self.fields.iter().any(|field| &field.name == unique_field) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "structured object unique key must reference an existing field",
                ));
            }
        }

        Ok(())
    }

    pub fn shape_hash(&self) -> ContractHash {
        let mut sink = ObjectShapeHashSink::new();
        sink.str("andromeda.catalog.structured-object-shape.v1");
        sink.object_ref(&self.object);
        sink.columns(&self.fields);
        sink.u64(self.unique_by.len() as u64);
        for field in &self.unique_by {
            sink.str(field);
        }
        sink.finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariant {
    pub name: String,
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDefinition {
    pub object: CatalogObjectRef,
    pub variants: Vec<EnumVariant>,
}

impl EnumDefinition {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.object.validate_for_definition(ObjectKind::Enum)?;

        if self.variants.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "enum definition must contain at least one variant",
            ));
        }

        let mut variant_names = BTreeSet::new();
        let mut variant_values = BTreeSet::new();
        for variant in &self.variants {
            if variant.name.trim().is_empty() {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "enum variant name must not be empty",
                ));
            }

            if !variant_names.insert(variant.name.as_str()) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "enum variant names must be unique",
                ));
            }

            if !variant_values.insert(variant.value) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "enum variant values must be unique",
                ));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogDefinition {
    Table(TableDefinition),
    StructuredObject(StructuredObjectDefinition),
    Enum(EnumDefinition),
    Procedure(ProcedureContract),
}

impl CatalogDefinition {
    pub fn object_ref(&self) -> &CatalogObjectRef {
        match self {
            Self::Table(definition) => &definition.object,
            Self::StructuredObject(definition) => &definition.object,
            Self::Enum(definition) => &definition.object,
            Self::Procedure(definition) => &definition.object,
        }
    }

    pub fn kind(&self) -> ObjectKind {
        match self {
            Self::Table(_) => ObjectKind::Table,
            Self::StructuredObject(_) => ObjectKind::StructuredObject,
            Self::Enum(_) => ObjectKind::Enum,
            Self::Procedure(_) => ObjectKind::Procedure,
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        match self {
            Self::Table(definition) => definition.validate(),
            Self::StructuredObject(definition) => definition.validate(),
            Self::Enum(definition) => definition.validate(),
            Self::Procedure(definition) => definition.validate_canonical_hash(),
        }
    }

    pub fn shape_hash(&self) -> ContractHash {
        match self {
            Self::Table(definition) => definition.shape_hash(),
            Self::StructuredObject(definition) => definition.shape_hash(),
            Self::Enum(definition) => {
                let mut sink = ObjectShapeHashSink::new();
                sink.str("andromeda.catalog.enum-shape.v1");
                sink.object_ref(&definition.object);
                sink.u64(definition.variants.len() as u64);
                for variant in &definition.variants {
                    sink.str(&variant.name);
                    sink.raw_bytes(&variant.value.to_le_bytes());
                }
                sink.finish()
            }
            Self::Procedure(definition) => definition.contract_hash,
        }
    }
}

struct ObjectShapeHashSink {
    lanes: [u64; 4],
}

impl ObjectShapeHashSink {
    fn new() -> Self {
        Self {
            lanes: [
                0xcbf29ce484222325,
                0x9e3779b97f4a7c15,
                0x517cc1b727220a95,
                0x6a09e667f3bcc909,
            ],
        }
    }

    fn finish(self) -> ContractHash {
        let mut bytes = [0u8; ContractHash::LEN];
        for (lane_index, lane) in self.lanes.into_iter().enumerate() {
            bytes[lane_index * 8..(lane_index + 1) * 8].copy_from_slice(&lane.to_le_bytes());
        }
        ContractHash::new(bytes)
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.u64(bytes.len() as u64);
        self.raw_bytes(bytes);
    }

    fn raw_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.u8(*byte);
        }
    }

    fn u8(&mut self, value: u8) {
        for (lane_index, lane) in self.lanes.iter_mut().enumerate() {
            *lane ^= u64::from(value).wrapping_add((lane_index as u64) << 8);
            *lane = lane.wrapping_mul(0x100000001b3 + (lane_index as u64 * 0x1000003d));
            *lane ^= lane.rotate_left(17 + lane_index as u32);
        }
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

    fn type_descriptor(&mut self, descriptor: &andromeda_core::TypeDescriptor) {
        self.scalar_type(&descriptor.scalar);
        self.u8(match descriptor.absence {
            andromeda_core::AbsencePolicy::Required => 0,
            andromeda_core::AbsencePolicy::ExplicitOptional => 1,
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

pub(crate) fn validate_columns(columns: &[ColumnDescriptor]) -> AndromedaResult<()> {
    validate_columns_with_min(columns, true)
}

pub(crate) fn validate_columns_allow_empty(columns: &[ColumnDescriptor]) -> AndromedaResult<()> {
    validate_columns_with_min(columns, false)
}

fn validate_columns_with_min(
    columns: &[ColumnDescriptor],
    require_non_empty: bool,
) -> AndromedaResult<()> {
    if require_non_empty && columns.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "column list must not be empty",
        ));
    }

    let mut column_names = BTreeSet::new();
    for (expected_ordinal, column) in columns.iter().enumerate() {
        column.validate()?;
        if !column_names.insert(column.name.as_str()) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "column names must be unique",
            ));
        }
        if column.ordinal != expected_ordinal as u32 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "column ordinals must be dense and zero-based",
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::{CatalogObjectId, ScalarType, TypeDescriptor};

    fn object(id: u64, name: &str, kind: ObjectKind, version: u64) -> CatalogObjectRef {
        CatalogObjectRef {
            object_id: CatalogObjectId::new(id),
            name: QualifiedName::parse(name).unwrap(),
            kind,
            catalog_version: CatalogVersion::new(version),
        }
    }

    #[test]
    fn catalog_object_binding_requires_exact_versioned_catalog_objects() {
        let binding = CatalogObjectBinding {
            dependent: object(1, "Inventory.ReserveStock", ObjectKind::Procedure, 7),
            dependency: object(2, "Inventory.ProductStock", ObjectKind::Table, 7),
            kind: CatalogBindingKind::WritesTable,
        };

        assert!(binding.validate().is_ok());
    }

    #[test]
    fn catalog_object_binding_rejects_wrong_dependency_kind() {
        let binding = CatalogObjectBinding {
            dependent: object(1, "Inventory.ReserveStock", ObjectKind::Procedure, 7),
            dependency: object(2, "Inventory.Reservation", ObjectKind::Table, 7),
            kind: CatalogBindingKind::EmitsStructuredObject,
        };

        let error = binding.validate().unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("kind"));
    }

    #[test]
    fn catalog_object_binding_rejects_version_skew() {
        let binding = CatalogObjectBinding {
            dependent: object(1, "Inventory.ReserveStock", ObjectKind::Procedure, 7),
            dependency: object(2, "Inventory.ProductStock", ObjectKind::Table, 8),
            kind: CatalogBindingKind::ReadsTable,
        };

        let error = binding.validate().unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("exact catalog version"));
    }

    #[test]
    fn table_and_structured_definition_validation_still_checks_columns() {
        let table = TableDefinition {
            object: object(2, "Inventory.ProductStock", ObjectKind::Table, 7),
            columns: vec![ColumnDescriptor {
                name: "ProductId".to_string(),
                data_type: TypeDescriptor::required(ScalarType::I64),
                ordinal: 0,
            }],
        };
        let structured = StructuredObjectDefinition {
            object: object(3, "Inventory.Reservation", ObjectKind::StructuredObject, 7),
            fields: table.columns.clone(),
            unique_by: vec!["ProductId".to_string()],
        };

        assert!(table.validate().is_ok());
        assert!(structured.validate().is_ok());
    }
}
