//! Procedure manifest DTOs owned by the catalog-store boundary.
//!
//! These types are runtime-free and protocol-free. Protobuf projection remains
//! outside this crate.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

/// A procedure manifest providing metadata needed by remote clients.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureManifest {
    pub procedure_id: ProcedureId,
    pub qualified_name: String,
    pub catalog_version: CatalogVersion,
    pub contract_hash: Vec<u8>,
    pub input_schema: Vec<ColumnSchema>,
    pub output_schema: Vec<ColumnSchema>,
    pub is_mutable: bool,
    pub min_compatible_version: CatalogVersion,
}

impl ProcedureManifest {
    /// Validate the manifest structure.
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.procedure_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "procedure id must not be zero",
            ));
        }

        if self.qualified_name.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "procedure qualified name must not be empty",
            ));
        }

        if self.catalog_version.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog version must not be zero",
            ));
        }

        if self.contract_hash.len() != ContractHash::LEN {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "procedure contract hash must be 32 bytes",
            ));
        }

        if self.contract_hash.iter().all(|byte| *byte == 0) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "procedure contract hash must not be zero",
            ));
        }

        if self.min_compatible_version.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "minimum compatible catalog version must not be zero",
            ));
        }

        if self.min_compatible_version.get() > self.catalog_version.get() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "minimum compatible catalog version must not exceed catalog version",
            ));
        }

        validate_schema_columns("input", &self.input_schema)?;
        validate_schema_columns("output", &self.output_schema)?;

        Ok(())
    }
}

/// A single column in a procedure's input or output schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnSchema {
    /// Column name.
    pub name: String,
    /// Type descriptor, for example `int64`, `text`, or `decimal(18,2)`.
    pub type_descriptor: String,
    /// Column ordinal, zero-based.
    pub ordinal: u32,
    /// Whether explicit optional absence values are permitted.
    pub nullable: bool,
}

impl ColumnSchema {
    /// Validate the column schema.
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.name.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "column name must not be empty",
            ));
        }

        if self.type_descriptor.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "column type descriptor must not be empty",
            ));
        }

        Ok(())
    }
}

fn validate_schema_columns(label: &'static str, columns: &[ColumnSchema]) -> AndromedaResult<()> {
    for (expected_ordinal, column) in columns.iter().enumerate() {
        column.validate()?;
        if column.ordinal as usize != expected_ordinal {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                format!("{label} schema column ordinals must be contiguous from zero"),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract_hash(byte: u8) -> Vec<u8> {
        vec![byte; ContractHash::LEN]
    }

    fn valid_manifest() -> ProcedureManifest {
        ProcedureManifest {
            procedure_id: ProcedureId::new(1),
            qualified_name: "public.my_proc".to_string(),
            catalog_version: CatalogVersion::new(2),
            contract_hash: contract_hash(0xA5),
            input_schema: vec![ColumnSchema {
                name: "input_col".to_string(),
                type_descriptor: "int64".to_string(),
                ordinal: 0,
                nullable: false,
            }],
            output_schema: vec![ColumnSchema {
                name: "result".to_string(),
                type_descriptor: "text".to_string(),
                ordinal: 0,
                nullable: true,
            }],
            is_mutable: false,
            min_compatible_version: CatalogVersion::new(1),
        }
    }

    #[test]
    fn procedure_manifest_validation_rejects_invalid_boundaries() {
        let valid = valid_manifest();
        assert!(valid.validate().is_ok());

        let invalid_id = ProcedureManifest {
            procedure_id: ProcedureId::new(0),
            ..valid.clone()
        };
        assert!(invalid_id.validate().is_err());

        let invalid_name = ProcedureManifest {
            qualified_name: " \t ".to_string(),
            ..valid.clone()
        };
        assert!(invalid_name.validate().is_err());

        let invalid_version = ProcedureManifest {
            catalog_version: CatalogVersion::new(0),
            ..valid.clone()
        };
        assert!(invalid_version.validate().is_err());

        let invalid_hash_len = ProcedureManifest {
            contract_hash: vec![],
            ..valid.clone()
        };
        assert!(invalid_hash_len.validate().is_err());

        let zero_hash = ProcedureManifest {
            contract_hash: contract_hash(0),
            ..valid.clone()
        };
        assert!(zero_hash.validate().is_err());

        let incompatible_floor = ProcedureManifest {
            min_compatible_version: CatalogVersion::new(3),
            ..valid.clone()
        };
        assert!(incompatible_floor.validate().is_err());

        let non_contiguous_input_schema = ProcedureManifest {
            input_schema: vec![ColumnSchema {
                name: "input_col".to_string(),
                type_descriptor: "int64".to_string(),
                ordinal: 1,
                nullable: false,
            }],
            ..valid
        };
        assert!(non_contiguous_input_schema.validate().is_err());
    }

    #[test]
    fn column_schema_validation_rejects_empty_name_or_type() {
        let valid = ColumnSchema {
            name: "id".to_string(),
            type_descriptor: "int64".to_string(),
            ordinal: 0,
            nullable: false,
        };
        assert!(valid.validate().is_ok());

        let invalid_name = ColumnSchema {
            name: " ".to_string(),
            ..valid.clone()
        };
        assert!(invalid_name.validate().is_err());

        let invalid_type = ColumnSchema {
            type_descriptor: "\n".to_string(),
            ..valid
        };
        assert!(invalid_type.validate().is_err());
    }
}
