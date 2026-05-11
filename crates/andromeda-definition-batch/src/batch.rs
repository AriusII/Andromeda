//! Portable DefinitionBatch descriptor.

use andromeda_catalog_store::CatalogDefinition;
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};

use crate::{
    DefinitionBatchDependencyGraphHash, DefinitionBatchId, DefinitionBatchSourceHash,
    DefinitionOperation, compute_definition_batch_source_hash, validate_in_batch_dependencies,
};

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

    /// Validates caller-provided integrity hashes against the current ordered
    /// DefinitionBatch source and dependency graph.
    pub fn validate_integrity_hashes(
        &self,
        source_hash: DefinitionBatchSourceHash,
        dependency_graph_hash: DefinitionBatchDependencyGraphHash,
    ) -> AndromedaResult<()> {
        if source_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "definition batch source hash must not be zero",
            ));
        }

        if dependency_graph_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "definition batch dependency graph hash must not be zero",
            ));
        }

        if source_hash != self.source_hash() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "definition batch source hash must match the current ordered batch source",
            ));
        }

        if dependency_graph_hash != self.dependency_graph_hash()? {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "definition batch dependency graph hash must match the current batch dependency graph",
            ));
        }

        Ok(())
    }

    pub fn definitions(&self) -> impl Iterator<Item = &CatalogDefinition> {
        self.operations
            .iter()
            .filter_map(|operation| match operation {
                DefinitionOperation::Create(definition) => Some(definition),
                DefinitionOperation::Deprecate(_) => None,
            })
    }
}
