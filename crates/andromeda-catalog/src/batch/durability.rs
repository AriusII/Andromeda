//! Durability evidence and publication receipt types for catalog mutations.

use andromeda_catalog_store::{CatalogPublicationCommitEvidence, CatalogPublicationPlan};
use andromeda_definition_batch::{
    DefinitionBatchDependencyGraphHash, DefinitionBatchId, DefinitionBatchSourceHash,
};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use super::mutation::{
    CatalogMutationBoundary, CatalogMutationPlan, CatalogMutationRecord,
    CatalogPublicationSemantics,
};

pub use andromeda_catalog_store::{CatalogDurabilityMarker, CatalogMutationDurability};

pub type CatalogPublicationReceipt = andromeda_catalog_store::CatalogPublicationReceipt<
    DefinitionBatchId,
    DefinitionBatchSourceHash,
    DefinitionBatchDependencyGraphHash,
>;

/// Proof that a specific commit record was durably persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogMutationCommitEvidence {
    commit_boundary: CatalogMutationBoundary,
    record_count: usize,
    durability: CatalogMutationDurability,
}

impl CatalogMutationCommitEvidence {
    pub fn from_durable_commit_record(
        record: &CatalogMutationRecord,
        record_count: usize,
        durability: CatalogMutationDurability,
    ) -> AndromedaResult<Self> {
        match record {
            CatalogMutationRecord::Commit(commit_boundary) => Ok(Self {
                commit_boundary: *commit_boundary,
                record_count,
                durability,
            }),
            CatalogMutationRecord::Begin(_) | CatalogMutationRecord::Apply(_) => {
                Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "catalog publication requires committed mutation evidence",
                ))
            },
        }
    }

    pub fn validate_for_plan(self, plan: &CatalogMutationPlan) -> AndromedaResult<()> {
        plan.validate_definition_batch_integrity()?;
        self.durability.validate()?;

        if !plan.mutation().is_monotonic() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog publication mutation plan must advance the catalog version",
            ));
        }

        if self.record_count != plan.record_count() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog publication record count must match the mutation plan",
            ));
        }

        if self.commit_boundary.batch_id != plan.batch_id
            || self.commit_boundary.database_id != plan.database_id
            || self.commit_boundary.namespace_id != plan.namespace_id
            || self.commit_boundary.previous_version != plan.previous_version
            || self.commit_boundary.next_version != plan.next_version
            || self.commit_boundary.source_hash != plan.source_hash
            || self.commit_boundary.dependency_graph_hash != plan.dependency_graph_hash
            || self.commit_boundary.publication_semantics != plan.publication_semantics
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog publication commit evidence must match the mutation plan boundary",
            ));
        }

        if self.commit_boundary.publication_semantics
            != CatalogPublicationSemantics::DurablePublicationExternal
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog publication requires durable publication semantics",
            ));
        }

        Ok(())
    }
}

impl
    CatalogPublicationPlan<
        DefinitionBatchId,
        DefinitionBatchSourceHash,
        DefinitionBatchDependencyGraphHash,
    > for CatalogMutationPlan
{
    fn batch_id(&self) -> DefinitionBatchId {
        self.batch_id
    }

    fn database_id(&self) -> andromeda_types::DatabaseId {
        self.database_id
    }

    fn namespace_id(&self) -> andromeda_types::NamespaceId {
        self.namespace_id
    }

    fn previous_version(&self) -> andromeda_types::CatalogVersion {
        self.previous_version
    }

    fn next_version(&self) -> andromeda_types::CatalogVersion {
        self.next_version
    }

    fn source_hash(&self) -> DefinitionBatchSourceHash {
        self.source_hash
    }

    fn dependency_graph_hash(&self) -> DefinitionBatchDependencyGraphHash {
        self.dependency_graph_hash
    }

    fn record_count(&self) -> usize {
        self.record_count()
    }

    fn publication_semantics(&self) -> CatalogPublicationSemantics {
        self.publication_semantics
    }

    fn is_monotonic(&self) -> bool {
        self.mutation().is_monotonic()
    }
}

impl CatalogPublicationCommitEvidence<CatalogMutationPlan> for CatalogMutationCommitEvidence {
    fn validate_for_publication_plan(&self, plan: &CatalogMutationPlan) -> AndromedaResult<()> {
        (*self).validate_for_plan(plan)
    }

    fn durable_lsn(&self) -> Option<u64> {
        self.durability.durable_lsn()
    }

    fn durable_evidence_marker(&self) -> Option<CatalogDurabilityMarker> {
        self.durability.durable_marker()
    }

    fn record_count(&self) -> usize {
        self.record_count
    }
}
