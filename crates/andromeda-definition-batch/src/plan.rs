//! Owner-level DefinitionBatch planning shape.

use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};

use crate::{DefinitionBatchId, PlannedDefinition, PlannedLifecycleTransition};

/// The validated, ready-to-apply result of DefinitionBatch planning.
///
/// Runtime crates supply the owner-specific mutation plan type while this crate
/// owns the common batch identity, version, and lifecycle evidence shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionBatchPlan<MutationPlan> {
    pub batch_id: DefinitionBatchId,
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub operation_count: usize,
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
    pub created_objects: Vec<PlannedDefinition>,
    pub deprecated_objects: Vec<PlannedLifecycleTransition>,
    pub mutation_plan: MutationPlan,
}
