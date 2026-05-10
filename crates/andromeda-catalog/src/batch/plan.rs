//! Catalog dry-run plan types produced by `DefinitionBatch::dry_run`.

use super::mutation::CatalogMutationPlan;

pub type DefinitionBatchPlan = andromeda_definition_batch::DefinitionBatchPlan<CatalogMutationPlan>;
