//! Durability evidence and publication receipt types for catalog mutations.

use andromeda_definition_batch::{DefinitionBatchId, DefinitionBatchSourceHash};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};

use super::mutation::{
    CatalogMutationBoundary, CatalogMutationPlan, CatalogMutationRecord,
    CatalogPublicationSemantics,
};
use crate::DefinitionBatchDependencyGraphHash;

/// An opaque monotonic marker from an external durable store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CatalogDurabilityMarker(u64);

impl CatalogDurabilityMarker {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Proof that a catalog mutation has been durably persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogMutationDurability {
    StorageWal { commit_lsn: u64, durable_lsn: u64 },
    ExternalMarker(CatalogDurabilityMarker),
}

impl CatalogMutationDurability {
    pub fn validate(self) -> AndromedaResult<()> {
        match self {
            Self::StorageWal {
                commit_lsn,
                durable_lsn,
            } => {
                if commit_lsn == 0 {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "catalog publication commit LSN must not be zero",
                    ));
                }
                if durable_lsn < commit_lsn {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "catalog publication durable LSN must reach the commit LSN",
                    ));
                }
            }
            Self::ExternalMarker(marker) => {
                if marker.get() == 0 {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "catalog publication durable evidence marker must not be zero",
                    ));
                }
            }
        }

        Ok(())
    }

    pub const fn durable_lsn(self) -> Option<u64> {
        match self {
            Self::StorageWal { durable_lsn, .. } => Some(durable_lsn),
            Self::ExternalMarker(_) => None,
        }
    }

    pub const fn durable_marker(self) -> Option<CatalogDurabilityMarker> {
        match self {
            Self::StorageWal { .. } => None,
            Self::ExternalMarker(marker) => Some(marker),
        }
    }
}

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
            }
        }
    }

    pub fn validate_for_plan(self, plan: &CatalogMutationPlan) -> AndromedaResult<()> {
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

/// Immutable confirmation that a catalog mutation was applied and durably persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogPublicationReceipt {
    pub batch_id: DefinitionBatchId,
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub previous_version: CatalogVersion,
    pub next_version: CatalogVersion,
    pub source_hash: DefinitionBatchSourceHash,
    pub dependency_graph_hash: DefinitionBatchDependencyGraphHash,
    pub durable_lsn: Option<u64>,
    pub durable_evidence_marker: Option<CatalogDurabilityMarker>,
    pub record_count: usize,
    pub publication_semantics: CatalogPublicationSemantics,
}

impl CatalogPublicationReceipt {
    pub fn from_plan_and_evidence(
        plan: &CatalogMutationPlan,
        evidence: CatalogMutationCommitEvidence,
    ) -> AndromedaResult<Self> {
        evidence.validate_for_plan(plan)?;

        Ok(Self {
            batch_id: plan.batch_id,
            database_id: plan.database_id,
            namespace_id: plan.namespace_id,
            previous_version: plan.previous_version,
            next_version: plan.next_version,
            source_hash: plan.source_hash,
            dependency_graph_hash: plan.dependency_graph_hash,
            durable_lsn: evidence.durability.durable_lsn(),
            durable_evidence_marker: evidence.durability.durable_marker(),
            record_count: evidence.record_count,
            publication_semantics: plan.publication_semantics,
        })
    }
}
