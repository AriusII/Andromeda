//! Catalog compatibility surface for DefinitionBatch.

use andromeda_definition_batch::dry_run_definition_batch;
pub use andromeda_definition_batch::{
    CatalogLifecycleTarget, DefinitionBatch, DefinitionOperation,
};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::CatalogVersion;

use super::mutation::{CatalogMutation, CatalogMutationDelta, CatalogMutationPlan};
use super::plan::DefinitionBatchPlan;

/// Catalog-local extension that turns a portable DefinitionBatch into a
/// runtime mutation/WAL plan.
pub trait CatalogDefinitionBatchPlanning {
    /// Validates all operations and produces a [`DefinitionBatchPlan`] without
    /// applying any mutations.
    fn dry_run(&self) -> AndromedaResult<DefinitionBatchPlan>;
}

impl CatalogDefinitionBatchPlanning for DefinitionBatch {
    fn dry_run(&self) -> AndromedaResult<DefinitionBatchPlan> {
        let dry_run = dry_run_definition_batch(
            self.batch_id,
            self.database_id,
            self.namespace_id,
            self.base_version,
            &self.operations,
        )?;

        let mutation = CatalogMutation {
            definition_batch_id: self.batch_id,
            previous_version: self.base_version,
            next_version: dry_run.next_version,
        };
        if !mutation.is_monotonic() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog mutation must advance the catalog version",
            ));
        }

        let mutation_plan = CatalogMutationPlan::new(
            self.batch_id,
            self.database_id,
            self.namespace_id,
            self.base_version,
            dry_run.next_version,
            dry_run.source_hash,
            dry_run.dependency_graph_hash,
            mutation_deltas(&self.operations, dry_run.next_version),
        )?;

        Ok(DefinitionBatchPlan {
            batch_id: dry_run.batch_id,
            database_id: dry_run.database_id,
            namespace_id: dry_run.namespace_id,
            operation_count: dry_run.operation_count,
            previous_version: dry_run.previous_version,
            next_version: dry_run.next_version,
            created_objects: dry_run.created_objects,
            deprecated_objects: dry_run.deprecated_objects,
            mutation_plan,
        })
    }
}

fn mutation_deltas(
    operations: &[DefinitionOperation],
    next_version: CatalogVersion,
) -> Vec<CatalogMutationDelta> {
    operations
        .iter()
        .enumerate()
        .map(|(operation_index, operation)| match operation {
            DefinitionOperation::Create(definition) => {
                CatalogMutationDelta::create(operation_index, next_version, definition.clone())
            },
            DefinitionOperation::Deprecate(target) => {
                CatalogMutationDelta::deprecate(operation_index, next_version, target.clone())
            },
        })
        .collect()
}
