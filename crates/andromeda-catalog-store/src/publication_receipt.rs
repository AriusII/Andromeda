//! Durable catalog publication receipt types owned by catalog-store.
//!
//! The receipt captures immutable durable publication evidence. Catalog-engine
//! adapters provide the concrete plan and commit-evidence validation.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};

use crate::CatalogPublicationSemantics;

/// Opaque security audit trace identifier carried by receipts that affect
/// permissions, policies, invocable procedures, or administrative catalog state.
///
/// Backed by a fixed-size byte array so the receipt remains `Copy`.
/// The admission integration layer (P05/P06 scope) is responsible for
/// generating non-zero values; all normal-path constructions default to `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SecurityAuditTraceId([u8; 32]);

impl SecurityAuditTraceId {
    /// Constructs a `SecurityAuditTraceId` from a raw 32-byte trace token.
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the underlying byte representation.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Decision context for the provenance of a publication receipt.
///
/// `NotRecovered` is the normal active-commit value.  `RecoveredFromDurableWal`
/// is set by the WAL-replay recovery path when it reconstructs receipts during
/// startup (populated in the P05/P06 admission-integration window).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogPublicationRecoveryDecision {
    /// Normal path: receipt was produced during an active durable commit, not recovery.
    NotRecovered,
    /// Recovery path: receipt was reconstructed from WAL replay at startup.
    RecoveredFromDurableWal {
        /// The durable LSN at which the commit record was observed during replay.
        source_lsn: u64,
    },
}

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

    /// Index of the first apply operation in this batch (always `0` for
    /// well-formed batches produced by `CatalogMutationPlan::new`).
    fn first_operation_index(&self) -> u32;

    /// Index of the last apply operation in this batch.
    ///
    /// For a plan with `N` deltas: `last = N − 1`.
    /// Combined with `first_operation_index`, this proves dense coverage:
    /// `record_count == (last − first + 1) + 2` (the `+2` accounts for the
    /// Begin and Commit boundary records).
    fn last_operation_index(&self) -> u32;
}

/// Store-facing validation hook for committed catalog mutation evidence.
pub trait CatalogPublicationCommitEvidence<Plan> {
    fn validate_for_publication_plan(&self, plan: &Plan) -> AndromedaResult<()>;

    fn durable_lsn(&self) -> Option<u64>;

    fn durable_evidence_marker(&self) -> Option<CatalogDurabilityMarker>;

    fn record_count(&self) -> usize;

    /// The provenance decision for this receipt.
    ///
    /// Normal active-commit path returns `NotRecovered`.  The WAL-replay
    /// recovery path returns `RecoveredFromDurableWal { source_lsn }`.
    fn recovery_decision(&self) -> CatalogPublicationRecoveryDecision;

    /// The security audit trace identifier, if the batch affected permissions,
    /// policies, invocable procedures, or administrative catalog state.
    ///
    /// Returns `None` on the normal path until the P05/P06 admission
    /// integration layer generates trace identifiers.
    fn security_audit_trace_id(&self) -> Option<SecurityAuditTraceId>;
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
    /// Index of the first apply operation in this batch (SPEC §226).
    ///
    /// Always `0` for well-formed batches. Proves the lower bound of dense
    /// operation coverage together with `last_operation_index`.
    pub first_operation_index: u32,
    /// Index of the last apply operation in this batch (SPEC §226).
    ///
    /// For a plan with `N` deltas: `last = N − 1`.  Together with
    /// `first_operation_index` this satisfies:
    /// `record_count == (last − first + 1) + 2`.
    pub last_operation_index: u32,
    /// Recovery provenance for this receipt (SPEC §230).
    ///
    /// `NotRecovered` on the normal durable-commit path.
    /// `RecoveredFromDurableWal` when reconstructed during WAL replay.
    pub recovery_decision: CatalogPublicationRecoveryDecision,
    /// Security audit trace identifier (SPEC §229).
    ///
    /// `None` until the P05/P06 admission-integration layer generates
    /// non-zero identifiers for batches that affect security-sensitive state.
    pub security_audit_trace_id: Option<SecurityAuditTraceId>,
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

        let first_operation_index = plan.first_operation_index();
        let last_operation_index = plan.last_operation_index();

        // GAP-2 (SPEC §226): prove dense operation coverage.
        if last_operation_index < first_operation_index {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog publication receipt last operation index must not precede first operation index",
            ));
        }

        // `op_span` is safe from overflow: last >= first guaranteed above.
        let op_span = (last_operation_index - first_operation_index + 1) as usize;
        let evidence_record_count = evidence.record_count();
        if evidence_record_count != op_span + 2 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog publication receipt record count must equal operation span plus two boundary records",
            ));
        }

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
            first_operation_index,
            last_operation_index,
            recovery_decision: evidence.recovery_decision(),
            security_audit_trace_id: evidence.security_audit_trace_id(),
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
        first_operation_index: u32,
        last_operation_index: u32,
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

        fn first_operation_index(&self) -> u32 {
            self.first_operation_index
        }

        fn last_operation_index(&self) -> u32 {
            self.last_operation_index
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

        fn recovery_decision(&self) -> CatalogPublicationRecoveryDecision {
            CatalogPublicationRecoveryDecision::NotRecovered
        }

        fn security_audit_trace_id(&self) -> Option<SecurityAuditTraceId> {
            None
        }
    }

    fn plan() -> TestPublicationPlan {
        TestPublicationPlan {
            record_count: 3,
            publication_semantics: CatalogPublicationSemantics::DurablePublicationExternal,
            monotonic: true,
            // record_count = 3 → 1 apply record → first=0, last=0
            first_operation_index: 0,
            last_operation_index: 0,
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
        // GAP-2 fields
        assert_eq!(receipt.first_operation_index, 0);
        assert_eq!(receipt.last_operation_index, 0);
        // GAP-1 fields
        assert_eq!(
            receipt.recovery_decision,
            CatalogPublicationRecoveryDecision::NotRecovered
        );
        assert_eq!(receipt.security_audit_trace_id, None);
    }

    // --- GAP-1 tests ---

    #[test]
    fn receipt_carries_security_audit_trace_id_field_with_none_default() {
        let receipt = CatalogPublicationReceipt::from_plan_and_evidence(
            &plan(),
            TestCommitEvidence {
                durability: CatalogMutationDurability::StorageWal {
                    commit_lsn: 10,
                    durable_lsn: 10,
                },
                record_count: 3,
            },
        )
        .unwrap();

        // P05/P06 admission integration has not run: field must exist and be None.
        assert_eq!(receipt.security_audit_trace_id, None);
    }

    #[test]
    fn receipt_recovery_decision_distinguishes_normal_commit_from_replay() {
        let receipt = CatalogPublicationReceipt::from_plan_and_evidence(
            &plan(),
            TestCommitEvidence {
                durability: CatalogMutationDurability::StorageWal {
                    commit_lsn: 10,
                    durable_lsn: 10,
                },
                record_count: 3,
            },
        )
        .unwrap();

        // Normal active-commit path must yield NotRecovered.
        assert_eq!(
            receipt.recovery_decision,
            CatalogPublicationRecoveryDecision::NotRecovered
        );

        // The RecoveredFromDurableWal variant round-trips its source_lsn.
        let recovered =
            CatalogPublicationRecoveryDecision::RecoveredFromDurableWal { source_lsn: 42 };
        assert_ne!(recovered, CatalogPublicationRecoveryDecision::NotRecovered);
        match recovered {
            CatalogPublicationRecoveryDecision::RecoveredFromDurableWal { source_lsn } => {
                assert_eq!(source_lsn, 42);
            },
            CatalogPublicationRecoveryDecision::NotRecovered => {
                panic!("expected RecoveredFromDurableWal");
            },
        }
    }

    // --- GAP-2 tests ---

    #[test]
    fn receipt_rejects_inconsistent_first_last_operation_index() {
        // last_operation_index < first_operation_index → structural violation.
        let bad_plan = TestPublicationPlan {
            record_count: 3,
            publication_semantics: CatalogPublicationSemantics::DurablePublicationExternal,
            monotonic: true,
            first_operation_index: 2,
            last_operation_index: 1, // last < first
        };

        let error = CatalogPublicationReceipt::from_plan_and_evidence(
            &bad_plan,
            TestCommitEvidence {
                durability: CatalogMutationDurability::StorageWal {
                    commit_lsn: 10,
                    durable_lsn: 10,
                },
                record_count: 3,
            },
        )
        .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("last operation index"));
    }

    #[test]
    fn receipt_rejects_record_count_not_matching_first_last_index_span() {
        // first=0, last=5 → op_span=6 → record_count must be 8, but evidence says 3.
        let bad_plan = TestPublicationPlan {
            record_count: 3,
            publication_semantics: CatalogPublicationSemantics::DurablePublicationExternal,
            monotonic: true,
            first_operation_index: 0,
            last_operation_index: 5,
        };

        let error = CatalogPublicationReceipt::from_plan_and_evidence(
            &bad_plan,
            TestCommitEvidence {
                durability: CatalogMutationDurability::StorageWal {
                    commit_lsn: 10,
                    durable_lsn: 10,
                },
                record_count: 3,
            },
        )
        .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("record count"));
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
