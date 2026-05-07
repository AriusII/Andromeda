//! Dry-run plan types produced by `DefinitionBatch::dry_run`.

use andromeda_types::{CatalogObjectId, CatalogVersion, DatabaseId, NamespaceId};

use crate::{ObjectKind, QualifiedName};

use super::definition::DefinitionBatchId;
use super::mutation::CatalogMutationPlan;

/// A successfully planned object creation within a [`DefinitionBatchPlan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedDefinition {
    pub object_id: CatalogObjectId,
    pub name: QualifiedName,
    pub kind: ObjectKind,
    pub planned_version: CatalogVersion,
}

/// The lifecycle action applied to an existing catalog object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogLifecycleAction {
    Deprecate,
}

/// A successfully planned lifecycle transition within a [`DefinitionBatchPlan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedLifecycleTransition {
    pub object_id: CatalogObjectId,
    pub name: QualifiedName,
    pub kind: ObjectKind,
    pub action: CatalogLifecycleAction,
    pub planned_version: CatalogVersion,
}

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
