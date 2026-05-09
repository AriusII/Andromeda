//! SRPL procedure operations within DefinitionBatch.
//!
//! This module extends the batch infrastructure to support SRPL procedures:
//! - **AddSrplProcedure**: Add a new SRPL procedure to the catalog
//! - **AlterSrplProcedure**: Replace an existing SRPL procedure's source (E2)
//! - **DropSrplProcedure**: Remove an SRPL procedure from the catalog (E3)
//!
//! All operations follow the batch execution model:
//! - Dry-run phase: Compile SRPL, validate IR, check references
//! - Apply phase: Store compiled manifest, update catalog version
//! - Atomic: All-or-nothing semantics (batch succeeds or fails as a unit)
//!
//! ## Integration with E2 (Alter) and E3 (Drop)
//!
//! - **ALTER**: Validates that procedure exists, compiles new source, preserves procedure_id
//! - **DROP**: Validates that procedure exists, checks no dependencies, marks deprecated
//!
//! See:
//! - DEC-022: Alter Procedure Lifecycle Semantics
//! - DEC-023: Drop Procedure Lifecycle Semantics

use andromeda_catalog_store::QualifiedName;
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::DefinitionBatch;

/// Add a new SRPL procedure to the batch.
///
/// On dry-run: Compiles SRPL source to IR, validates references.
/// On apply: Stores compiled manifest in catalog.
///
/// # Arguments
///
/// * `batch` - The batch to which the procedure is added
/// * `name` - Qualified name (e.g., "Inventory.ReserveStock")
/// * `source` - SRPL source code
///
/// # Errors
///
/// Returns an error if:
/// - Procedure name is not unique within the namespace
/// - SRPL source has syntax errors
/// - SRPL binding/lowering fails
/// - Procedure references are unresolved
pub fn add_srpl_procedure(
    _batch: &mut DefinitionBatch,
    _name: QualifiedName,
    _source: String,
) -> AndromedaResult<()> {
    Err(srpl_source_integration_error("add SRPL procedure"))
}

/// Alter an existing SRPL procedure's source (E2).
///
/// On dry-run: Compiles new SRPL source, validates compatibility.
/// On apply: Replaces procedure definition, bumps catalog version.
///
/// # Arguments
///
/// * `batch` - The batch containing the alteration
/// * `name` - Qualified name of procedure to alter
/// * `new_source` - New SRPL source code
///
/// # Errors
///
/// Returns an error if:
/// - Procedure does not exist
/// - New SRPL source has syntax errors
/// - New source is incompatible with existing contract
/// - ALTER would break dependent Maps
pub fn alter_srpl_procedure(
    _batch: &mut DefinitionBatch,
    _name: QualifiedName,
    _new_source: String,
) -> AndromedaResult<()> {
    Err(srpl_source_integration_error("alter SRPL procedure"))
}

/// Drop an existing SRPL procedure (E3).
///
/// On dry-run: Validates procedure exists, checks dependencies.
/// On apply: Marks procedure deprecated, bumps catalog version.
///
/// # Arguments
///
/// * `batch` - The batch containing the drop
/// * `name` - Qualified name of procedure to drop
///
/// # Errors
///
/// Returns an error if:
/// - Procedure does not exist
/// - Procedure has outstanding invocations
/// - Procedure is referenced by existing Maps
pub fn drop_srpl_procedure(
    _batch: &mut DefinitionBatch,
    _name: QualifiedName,
) -> AndromedaResult<()> {
    Err(srpl_source_integration_error("drop SRPL procedure"))
}

fn srpl_source_integration_error(operation: &str) -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Catalog,
        format!(
            "{operation} must be performed through the SRPL DefinitionBatch bridge; \
             andromeda-catalog cannot parse SRPL source without creating a crate dependency cycle"
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DefinitionBatchId;
    use andromeda_business_fixtures::{INVENTORY_DATABASE_ID, INVENTORY_NAMESPACE_ID};
    use andromeda_types::CatalogVersion;

    fn empty_batch() -> DefinitionBatch {
        DefinitionBatch {
            batch_id: DefinitionBatchId::new(1),
            database_id: INVENTORY_DATABASE_ID,
            namespace_id: INVENTORY_NAMESPACE_ID,
            base_version: CatalogVersion::new(0),
            operations: Vec::new(),
        }
    }

    #[test]
    fn srpl_source_helpers_do_not_silently_succeed_inside_catalog() {
        let mut batch = empty_batch();
        let error = add_srpl_procedure(
            &mut batch,
            QualifiedName::parse("Inventory.ReserveStock").unwrap(),
            "procedure Inventory.ReserveStock accepts () returns R one (Ok bool);".to_string(),
        )
        .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("SRPL DefinitionBatch bridge"));
        assert!(batch.operations.is_empty());
    }
}
