//! Catalog dry-run plan types produced by `DefinitionBatch::dry_run`.

use andromeda_definition_batch::DefinitionBatchId;
pub use andromeda_definition_batch::{
    CatalogLifecycleAction, PlannedDefinition, PlannedLifecycleTransition,
};
use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};

use super::mutation::CatalogMutationPlan;

/// The validated, ready-to-apply result of `DefinitionBatch::dry_run`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionBatchPlan {
    pub batch_id: DefinitionBatchId,
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub operation_count: usize,
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
    pub created_objects: Vec<PlannedDefinition>,
    pub deprecated_objects: Vec<PlannedLifecycleTransition>,
    pub mutation_plan: CatalogMutationPlan,
}
