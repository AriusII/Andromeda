use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, ProcedureId,
};

/// A procedure manifest providing metadata needed by remote clients.
///
/// Contains the essential information for resolving and executing procedures
/// without requiring ad hoc SQL queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureManifest {
    /// The procedure ID
    pub procedure_id: ProcedureId,
    /// The qualified name (e.g., "schema.procedure_name")
    pub qualified_name: String,
    /// The catalog version at which this procedure is defined
    pub catalog_version: CatalogVersion,
    /// Stable hash of the procedure contract for compatibility checking
    pub contract_hash: Vec<u8>,
    /// Input parameter names and types (schema)
    pub input_schema: Vec<ColumnSchema>,
    /// Output column names and types
    pub output_schema: Vec<ColumnSchema>,
    /// Whether this procedure may modify data
    pub is_mutable: bool,
    /// Minimum catalog version required for compatibility
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

        if self.qualified_name.is_empty() {
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

        if self.contract_hash.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "contract hash must not be empty",
            ));
        }

        Ok(())
    }
}

/// A single column in a procedure's input or output schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnSchema {
    /// Column name
    pub name: String,
    /// SQL type descriptor (e.g., "int64", "text", "decimal(18,2)")
    pub type_descriptor: String,
    /// Column ordinal (0-based)
    pub ordinal: u32,
    /// Whether NULL values are permitted
    pub nullable: bool,
}

impl ColumnSchema {
    /// Validate the column schema.
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.name.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "column name must not be empty",
            ));
        }

        if self.type_descriptor.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "column type descriptor must not be empty",
            ));
        }

        Ok(())
    }
}

/// A change notification for catalog updates.
///
/// Emitted when a procedure or other catalog object changes,
/// allowing clients to invalidate caches and revalidate procedure contracts.
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
    /// Determine if the change affected a specific procedure.
    /// In a real implementation, this would consult the change journal.
    pub fn affects_procedure(&self, _procedure_id: ProcedureId) -> bool {
        // In mock implementation, any version change affects all procedures
        // Real implementation would consult change tracking metadata
        self.new_version != self.previous_version
    }
}
