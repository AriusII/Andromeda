use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
    ColumnDescriptor,
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
            Self::Procedure(definition) => definition.validate(),
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
