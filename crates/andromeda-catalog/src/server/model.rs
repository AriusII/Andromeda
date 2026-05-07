use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, ContractHash, ProcedureId,
};

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
    /// Column name
    pub name: String,
    /// Type descriptor, for example "int64", "text", or "decimal(18,2)"
    pub type_descriptor: String,
    /// Column ordinal (0-based)
    pub ordinal: u32,
    /// Whether explicit optional absence values are permitted
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

/// A change notification for catalog updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogChangeNotification {
    /// The new catalog version after the change
    pub new_version: CatalogVersion,
    /// The previous catalog version
    pub previous_version: CatalogVersion,
    /// Log sequence number boundary for LSN-aware invalidation
    pub invalidation_boundary_lsn: u64,
}

impl CatalogChangeNotification {
    pub fn validate_version_order(&self) -> AndromedaResult<()> {
        if self.previous_version.get() == 0 || self.new_version.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog change notification versions must not be zero",
            ));
        }
        if self.new_version <= self.previous_version {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog change notification must advance catalog version ordering",
            ));
        }
        if self.invalidation_boundary_lsn == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog change notification invalidation boundary LSN must not be zero",
            ));
        }
        let expected_next = self.previous_version.get().checked_add(1).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog change notification version ordering overflowed",
            )
        })?;
        if self.new_version.get() != expected_next {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog change notification version ordering must advance by one catalog version",
            ));
        }
        Ok(())
    }

    /// Determine if the change affected a specific procedure.
    pub fn affects_procedure(&self, _procedure_id: ProcedureId) -> bool {
        self.new_version != self.previous_version
    }
}
