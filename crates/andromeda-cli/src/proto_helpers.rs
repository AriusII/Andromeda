//! Protocol helpers and utilities.

use andromeda_catalog::{CatalogDefinitionBatchPlanning, CatalogSnapshot};
use andromeda_error::AndromedaResult;
use andromeda_inventory_demo::{
    INVENTORY_DATABASE_ID, INVENTORY_NAMESPACE_ID, inventory_domain_definition_batch,
};

/// Creates a catalog snapshot for inventory domain.
pub fn inventory_catalog_snapshot() -> AndromedaResult<CatalogSnapshot> {
    let batch = inventory_domain_definition_batch()?;
    let plan = batch.dry_run()?;
    let mut snapshot = CatalogSnapshot::empty(
        INVENTORY_DATABASE_ID,
        INVENTORY_NAMESPACE_ID,
        batch.base_version,
    );
    snapshot.apply_mutation_plan(&plan.mutation_plan)?;
    Ok(snapshot)
}
