//! Catalog dry-run plan types produced by `DefinitionBatch::dry_run`.

pub use andromeda_definition_batch::{
    CatalogLifecycleAction, PlannedDefinition, PlannedLifecycleTransition,
};

use super::mutation::CatalogMutationPlan;

pub type DefinitionBatchPlan = andromeda_definition_batch::DefinitionBatchPlan<CatalogMutationPlan>;
