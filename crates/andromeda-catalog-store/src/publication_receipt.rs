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

#[cfg(test)]
mod tests {
    use super::*;

    const DATABASE_ID: DatabaseId = DatabaseId::new(1);
    const NAMESPACE_ID: NamespaceId = NamespaceId::new(2);
    const SOURCE_HASH: [u8; 32] = [0xA5; 32];
    const DEPENDENCY_GRAPH_HASH: [u8; 32] = [0xC3; 32];

    #[derive(Debug, Clone, Copy)]
    struct TestPublicationPlan {
        record_count: usize,
        publication_semantics: CatalogPublicationSemantics,
        monotonic: bool,
    }

    impl CatalogPublicationPlan<u64, [u8; 32], [u8; 32]> for TestPublicationPlan {
        fn batch_id(&self) -> u64 {
            101
        }

        fn database_id(&self) -> DatabaseId {
            DATABASE_ID
        }

        fn namespace_id(&self) -> NamespaceId {
            NAMESPACE_ID
        }

        fn previous_version(&self) -> CatalogVersion {
            CatalogVersion::new(10)
        }

        fn next_version(&self) -> CatalogVersion {
            CatalogVersion::new(11)
        }

        fn source_hash(&self) -> [u8; 32] {
            SOURCE_HASH
        }

        fn dependency_graph_hash(&self) -> [u8; 32] {
            DEPENDENCY_GRAPH_HASH
        }

        fn record_count(&self) -> usize {
            self.record_count
        }

        fn publication_semantics(&self) -> CatalogPublicationSemantics {
            self.publication_semantics
        }

        fn is_monotonic(&self) -> bool {
            self.monotonic
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct TestCommitEvidence {
        durability: CatalogMutationDurability,
        record_count: usize,
    }

    impl CatalogPublicationCommitEvidence<TestPublicationPlan> for TestCommitEvidence {
        fn validate_for_publication_plan(&self, plan: &TestPublicationPlan) -> AndromedaResult<()> {
            if !plan.is_monotonic() {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "test publication plan must be monotonic",
                ));
            }

            if self.record_count != plan.record_count() {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Catalog,
                    "test publication evidence record count mismatch",
                ));
            }

            self.durability.validate()
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

    fn plan() -> TestPublicationPlan {
        TestPublicationPlan {
            record_count: 3,
            publication_semantics: CatalogPublicationSemantics::DurablePublicationExternal,
            monotonic: true,
        }
    }

    #[test]
    fn mutation_durability_rejects_missing_or_non_durable_evidence() {
        let zero_commit = CatalogMutationDurability::StorageWal {
            commit_lsn: 0,
            durable_lsn: 10,
        };
        assert_eq!(
            zero_commit.validate().unwrap_err().kind(),
            AndromedaErrorKind::Catalog
        );

        let stale_durable_lsn = CatalogMutationDurability::StorageWal {
            commit_lsn: 10,
            durable_lsn: 9,
        };
        assert_eq!(
            stale_durable_lsn.validate().unwrap_err().kind(),
            AndromedaErrorKind::Catalog
        );

        let zero_marker =
            CatalogMutationDurability::ExternalMarker(CatalogDurabilityMarker::new(0));
        assert_eq!(
            zero_marker.validate().unwrap_err().kind(),
            AndromedaErrorKind::Catalog
        );

        assert!(
            CatalogMutationDurability::StorageWal {
                commit_lsn: 10,
                durable_lsn: 10
            }
            .validate()
            .is_ok()
        );
        assert!(
            CatalogMutationDurability::ExternalMarker(CatalogDurabilityMarker::new(44))
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn publication_receipt_copies_plan_fields_after_evidence_validation() {
        let receipt = CatalogPublicationReceipt::from_plan_and_evidence(
            &plan(),
            TestCommitEvidence {
                durability: CatalogMutationDurability::StorageWal {
                    commit_lsn: 77,
                    durable_lsn: 80,
                },
                record_count: 3,
            },
        )
        .unwrap();

        assert_eq!(receipt.batch_id, 101);
        assert_eq!(receipt.database_id, DATABASE_ID);
        assert_eq!(receipt.namespace_id, NAMESPACE_ID);
        assert_eq!(receipt.previous_version, CatalogVersion::new(10));
        assert_eq!(receipt.next_version, CatalogVersion::new(11));
        assert_eq!(receipt.source_hash, SOURCE_HASH);
        assert_eq!(receipt.dependency_graph_hash, DEPENDENCY_GRAPH_HASH);
        assert_eq!(receipt.durable_lsn, Some(80));
        assert_eq!(receipt.durable_evidence_marker, None);
        assert_eq!(receipt.record_count, 3);
        assert_eq!(
            receipt.publication_semantics,
            CatalogPublicationSemantics::DurablePublicationExternal
        );
    }

    #[test]
    fn publication_receipt_rejects_invalid_evidence_before_publication() {
        let error = CatalogPublicationReceipt::from_plan_and_evidence(
            &plan(),
            TestCommitEvidence {
                durability: CatalogMutationDurability::StorageWal {
                    commit_lsn: 77,
                    durable_lsn: 76,
                },
                record_count: 3,
            },
        )
        .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);

        let count_error = CatalogPublicationReceipt::from_plan_and_evidence(
            &plan(),
            TestCommitEvidence {
                durability: CatalogMutationDurability::ExternalMarker(
                    CatalogDurabilityMarker::new(44),
                ),
                record_count: 2,
            },
        )
        .unwrap_err();

        assert_eq!(count_error.kind(), AndromedaErrorKind::Catalog);
        assert!(count_error.message().contains("record count"));
    }
}
