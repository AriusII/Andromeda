//! Catalog compatibility facade for DefinitionBatch.

pub use andromeda_definition_batch::{CatalogLifecycleTarget, DefinitionOperation};
use andromeda_definition_batch::{
    DefinitionBatchId, DefinitionBatchSourceHash, compute_definition_batch_source_hash,
    dry_run_definition_batch, validate_in_batch_dependencies,
};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};

use super::mutation::{CatalogMutation, CatalogMutationDelta, CatalogMutationPlan};
use super::plan::DefinitionBatchPlan;
use crate::DefinitionBatchDependencyGraphHash;

/// An ordered, atomic set of catalog object definition operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionBatch {
    pub batch_id: DefinitionBatchId,
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub base_version: CatalogVersion,
    pub operations: Vec<DefinitionOperation>,
}

impl DefinitionBatch {
    /// Computes a deterministic digest over the exact ordered batch source.
    ///
    /// This hash intentionally includes operation order and object shape
    /// hashes. It complements [`DefinitionBatchDependencyGraphHash`], which
    /// canonicalizes the dependency graph and therefore ignores source order
    /// when the graph itself is equivalent.
    pub fn source_hash(&self) -> DefinitionBatchSourceHash {
        compute_definition_batch_source_hash(
            self.batch_id,
            self.database_id,
            self.namespace_id,
            self.base_version,
            &self.operations,
        )
    }

    /// Validates and computes the canonical dependency-graph digest for this
    /// batch.
    pub fn dependency_graph_hash(&self) -> AndromedaResult<DefinitionBatchDependencyGraphHash> {
        validate_in_batch_dependencies(&self.operations).map(|graph| graph.dependency_graph_hash())
    }

    /// Validates all operations and produces a [`DefinitionBatchPlan`] without
    /// applying any mutations.
    pub fn dry_run(&self) -> AndromedaResult<DefinitionBatchPlan> {
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
            }
            DefinitionOperation::Deprecate(target) => {
                CatalogMutationDelta::deprecate(operation_index, next_version, target.clone())
            }
        })
        .collect()
}
