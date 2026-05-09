//! Durable catalog publication receipt types owned by catalog-store.
//!
//! The receipt captures immutable durable publication evidence. Catalog-engine
//! adapters provide the concrete plan and commit-evidence validation.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};

use crate::CatalogPublicationSemantics;

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
            },
            Self::ExternalMarker(marker) => {
                if marker.get() == 0 {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Catalog,
                        "catalog publication durable evidence marker must not be zero",
                    ));
                }
            },
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

/// Store-facing view of a catalog mutation plan that can be published.
pub trait CatalogPublicationPlan<
    DefinitionBatchId,
    DefinitionBatchSourceHash,
    DefinitionBatchDependencyGraphHash,
>
{
    fn batch_id(&self) -> DefinitionBatchId;

    fn database_id(&self) -> DatabaseId;

    fn namespace_id(&self) -> NamespaceId;

    fn previous_version(&self) -> CatalogVersion;

    fn next_version(&self) -> CatalogVersion;

    fn source_hash(&self) -> DefinitionBatchSourceHash;

    fn dependency_graph_hash(&self) -> DefinitionBatchDependencyGraphHash;

    fn record_count(&self) -> usize;

    fn publication_semantics(&self) -> CatalogPublicationSemantics;

    fn is_monotonic(&self) -> bool;
}

/// Store-facing validation hook for committed catalog mutation evidence.
pub trait CatalogPublicationCommitEvidence<Plan> {
    fn validate_for_publication_plan(&self, plan: &Plan) -> AndromedaResult<()>;

    fn durable_lsn(&self) -> Option<u64>;

    fn durable_evidence_marker(&self) -> Option<CatalogDurabilityMarker>;

    fn record_count(&self) -> usize;
}

/// Immutable confirmation that a catalog mutation was applied and durably persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogPublicationReceipt<
    DefinitionBatchId,
    DefinitionBatchSourceHash,
    DefinitionBatchDependencyGraphHash,
> {
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

impl<DefinitionBatchId, DefinitionBatchSourceHash, DefinitionBatchDependencyGraphHash>
    CatalogPublicationReceipt<
        DefinitionBatchId,
        DefinitionBatchSourceHash,
        DefinitionBatchDependencyGraphHash,
    >
where
    DefinitionBatchId: Copy,
    DefinitionBatchSourceHash: Copy,
    DefinitionBatchDependencyGraphHash: Copy,
{
    pub fn from_plan_and_evidence<Plan, Evidence>(
        plan: &Plan,
        evidence: Evidence,
    ) -> AndromedaResult<Self>
    where
        Plan: CatalogPublicationPlan<
                DefinitionBatchId,
                DefinitionBatchSourceHash,
                DefinitionBatchDependencyGraphHash,
            >,
        Evidence: CatalogPublicationCommitEvidence<Plan>,
    {
        evidence.validate_for_publication_plan(plan)?;

        Ok(Self {
            batch_id: plan.batch_id(),
            database_id: plan.database_id(),
            namespace_id: plan.namespace_id(),
            previous_version: plan.previous_version(),
            next_version: plan.next_version(),
            source_hash: plan.source_hash(),
            dependency_graph_hash: plan.dependency_graph_hash(),
            durable_lsn: evidence.durable_lsn(),
            durable_evidence_marker: evidence.durable_evidence_marker(),
            record_count: evidence.record_count(),
            publication_semantics: plan.publication_semantics(),
        })
    }
}
