//! Catalog mutation records, plans, and WAL-boundary types.

use andromeda_definition_batch::{
    CatalogLifecycleTarget, DefinitionBatchDependencyGraphHash, DefinitionBatchId,
    DefinitionBatchSourceHash,
};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};
use std::collections::BTreeSet;

pub use andromeda_catalog_recovery::{
    CatalogMutationRecordKind, CatalogWalPayloadDecodeError, CatalogWalPayloadDecodeErrorKind,
};
pub use andromeda_catalog_store::{
    CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH, CatalogPublicationSemantics,
};

pub type CatalogMutation = andromeda_catalog_store::CatalogMutation<DefinitionBatchId>;

pub type CatalogMutationBoundary = andromeda_catalog_store::CatalogMutationBoundary<
    DefinitionBatchId,
    DefinitionBatchSourceHash,
    DefinitionBatchDependencyGraphHash,
>;

pub type CatalogMutationDelta =
    andromeda_catalog_store::CatalogMutationDelta<CatalogLifecycleTarget>;

pub type CatalogMutationOperation =
    andromeda_catalog_store::CatalogMutationOperation<CatalogLifecycleTarget>;

/// A WAL record emitted during catalog mutation replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogMutationRecord {
    Begin(CatalogMutationBoundary),
    Apply(Box<CatalogMutationDelta>),
    Commit(CatalogMutationBoundary),
}

impl CatalogMutationRecord {
    pub fn kind(&self) -> CatalogMutationRecordKind {
        match self {
            Self::Begin(_) => CatalogMutationRecordKind::CatalogChangeBegin,
            Self::Apply(_) => CatalogMutationRecordKind::CatalogChangeApply,
            Self::Commit(_) => CatalogMutationRecordKind::CatalogChangeCommit,
        }
    }
}

/// The ordered, validated set of deltas that will be applied atomically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogMutationPlan {
    pub batch_id: DefinitionBatchId,
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
    pub source_hash: DefinitionBatchSourceHash,
    pub dependency_graph_hash: DefinitionBatchDependencyGraphHash,
    pub publication_semantics: CatalogPublicationSemantics,
    pub deltas: Vec<CatalogMutationDelta>,
}

impl CatalogMutationPlan {
    #[allow(
        clippy::too_many_arguments,
        reason = "Catalog mutation plans keep versioned DefinitionBatch identities explicit."
    )]
    pub fn new(
        batch_id: DefinitionBatchId,
        database_id: DatabaseId,
        namespace_id: NamespaceId,
        previous_version: CatalogVersion,
        next_version: CatalogVersion,
        source_hash: DefinitionBatchSourceHash,
        dependency_graph_hash: DefinitionBatchDependencyGraphHash,
        deltas: Vec<CatalogMutationDelta>,
    ) -> AndromedaResult<Self> {
        if deltas.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog mutation plan must contain at least one delta",
            ));
        }
        if deltas.len() > CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                format!(
                    "catalog mutation plan must not exceed {CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH} apply records"
                ),
            ));
        }
        if batch_id.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog mutation plan batch id must not be zero",
            ));
        }
        if database_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog mutation plan database id must not be zero",
            ));
        }
        if namespace_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog mutation plan namespace id must not be zero",
            ));
        }

        let mutation = CatalogMutation {
            definition_batch_id: batch_id,
            previous_version,
            next_version,
        };
        if !mutation.is_monotonic() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog mutation plan must advance the catalog version",
            ));
        }
        if source_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog mutation plan source hash must not be zero",
            ));
        }
        if dependency_graph_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog mutation plan dependency graph hash must not be zero",
            ));
        }

        let mut object_ids = BTreeSet::new();
        let mut object_names = BTreeSet::new();
        for (expected_index, delta) in deltas.iter().enumerate() {
            if delta.operation_index != expected_index {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "catalog mutation deltas must be dense and operation ordered",
                ));
            }

            if delta.planned_version != next_version {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "catalog mutation delta version must match the planned next catalog version",
                ));
            }

            match &delta.operation {
                CatalogMutationOperation::CreateObject { object, definition } => {
                    definition.validate()?;
                    if object.catalog_version != next_version {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog mutation object version must match the planned next catalog version",
                        ));
                    }

                    if !object_ids.insert(object.object_id) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog mutation plan must not change the same object id twice",
                        ));
                    }

                    if !object_names.insert(object.name.clone()) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog mutation plan must not change the same object name twice",
                        ));
                    }
                },
                CatalogMutationOperation::DeprecateObject { target } => {
                    target.validate()?;
                    if target.object.catalog_version > previous_version {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog mutation lifecycle target version must not be newer than the previous catalog version",
                        ));
                    }

                    if !object_ids.insert(target.object.object_id) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog mutation plan must not change the same object id twice",
                        ));
                    }

                    if !object_names.insert(target.object.name.clone()) {
                        return Err(AndromedaError::new(
                            AndromedaErrorKind::Catalog,
                            "catalog mutation plan must not change the same object name twice",
                        ));
                    }
                },
            }
        }

        Ok(Self {
            batch_id,
            database_id,
            namespace_id,
            previous_version,
            next_version,
            source_hash,
            dependency_graph_hash,
            publication_semantics: CatalogPublicationSemantics::DurablePublicationExternal,
            deltas,
        })
    }

    /// Emits the full ordered sequence of WAL records for this plan.
    pub fn records(&self) -> Vec<CatalogMutationRecord> {
        let mut records = Vec::with_capacity(self.deltas.len() + 2);
        let boundary = self.commit_boundary();

        records.push(CatalogMutationRecord::Begin(boundary));
        records.extend(
            self.deltas
                .iter()
                .cloned()
                .map(|delta| CatalogMutationRecord::Apply(Box::new(delta))),
        );
        records.push(CatalogMutationRecord::Commit(boundary));
        records
    }

    /// Returns the total number of WAL records this plan emits (deltas + Begin + Commit).
    pub fn record_count(&self) -> usize {
        self.deltas.len() + 2
    }

    /// Returns the [`CatalogMutationBoundary`] shared by the Begin and Commit records.
    pub fn commit_boundary(&self) -> CatalogMutationBoundary {
        CatalogMutationBoundary {
            batch_id: self.batch_id,
            database_id: self.database_id,
            namespace_id: self.namespace_id,
            previous_version: self.previous_version,
            next_version: self.next_version,
            source_hash: self.source_hash,
            dependency_graph_hash: self.dependency_graph_hash,
            expected_apply_count: self.deltas.len(),
            publication_semantics: self.publication_semantics,
        }
    }

    /// Returns the [`CatalogMutation`] identity for this plan.
    pub fn mutation(&self) -> CatalogMutation {
        CatalogMutation {
            definition_batch_id: self.batch_id,
            previous_version: self.previous_version,
            next_version: self.next_version,
        }
    }
}
