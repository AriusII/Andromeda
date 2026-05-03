use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
    ColumnDescriptor,
};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDefinition {
    pub object: CatalogObjectRef,
    pub columns: Vec<ColumnDescriptor>,
}

impl TableDefinition {
    pub fn validate(&self) -> AndromedaResult<()> {
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
        if self.variants.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "enum definition must contain at least one variant",
            ));
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
    if columns.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "column list must not be empty",
        ));
    }

    for (expected_ordinal, column) in columns.iter().enumerate() {
        column.validate()?;
        if column.ordinal != expected_ordinal as u32 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "column ordinals must be dense and zero-based",
            ));
        }
    }

    Ok(())
}
