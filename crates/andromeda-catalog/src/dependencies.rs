//! Compatibility facade for DefinitionBatch dependency graph helpers.
//!
//! The portable graph and hash model lives in `andromeda-definition-batch`.
//! `andromeda-catalog` reexports it so existing catalog callers keep the same
//! public surface while the owner crate stays runtime-free.

pub use andromeda_contract::{CatalogDependency, CatalogDependencyKind};
pub use andromeda_definition_batch::{
    BatchDependencyGraph, DefinitionBatchDependencyGraphHash, validate_in_batch_dependencies,
    validate_in_batch_dependencies_with_bindings,
};
