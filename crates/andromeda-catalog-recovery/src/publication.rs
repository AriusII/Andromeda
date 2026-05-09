use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

mod acknowledgement;
mod audience;
mod audit;
mod invalidation;
mod published_object;
mod receipt_view;
mod replay;
mod report;
mod runtime;
mod subscriber;

pub use acknowledgement::{
    CatalogPublicationReceiptExpectation, CatalogSubscriptionAcknowledgement,
    validate_subscription_acknowledgement_for_publication,
};
pub use audience::{CatalogPublicationAudience, CatalogPublicationReasonCode};
pub use audit::{
    CatalogPublicationAuditTrace, CatalogVisibleChangeAuditEvidence,
    catalog_visible_change_audit_evidence_for_publication,
    validate_visible_change_audit_for_publication,
};
pub use invalidation::{
    CatalogPlanInvalidatedContract, CatalogPlanInvalidationReport, CatalogPublishedContract,
};
pub use published_object::CatalogPublishedObject;
pub use receipt_view::{
    CatalogPublicationBatchIdView, CatalogPublicationHashEvidence, CatalogPublicationReceiptView,
};
pub(crate) use replay::PublishedVersionKey;
pub use replay::{
    CatalogPublicationReplayKey, CatalogPublicationReplayTerminalOutcome,
    CatalogPublicationReplayTerminalRecord, CatalogPublicationSubscriptionReplayEvidence,
    CatalogPublicationSubscriptionReplayRecord, CatalogPublicationSubscriptionReplayRecordKind,
    CatalogPublicationSubscriptionReplaySummary, CatalogSubscriptionReplayKey,
    replay_publication_subscription_changes,
};
pub use report::{
    CatalogPublicationReport, CatalogRecoveryReplayExpectation,
    validate_catalog_publication_receipt,
};
pub use runtime::{
    CatalogHadrSubscriberReplayEvidence, CatalogPublicationRuntimeState,
    CatalogPublicationSubscriberRegistry, CatalogSubscriberAckProgress,
};
pub use subscriber::{
    CatalogSubscriberId, CatalogSubscriberIdentity, CatalogSubscriberKind,
    CatalogSubscriberRegistration,
};

pub(crate) fn require_equal<T: PartialEq + ?Sized>(
    observed: &T,
    expected: &T,
    message: &'static str,
) -> AndromedaResult<()> {
    if observed == expected {
        Ok(())
    } else {
        catalog_recovery_publication_error(message)
    }
}

pub(crate) fn catalog_recovery_publication_error<T>(message: &'static str) -> AndromedaResult<T> {
    Err(AndromedaError::new(AndromedaErrorKind::Catalog, message))
}

#[cfg(test)]
mod tests {
    use andromeda_error::AndromedaErrorKind;
    use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};

    use crate::CatalogPublicationSemantics;

    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct TestReceipt {
        batch_id: u64,
        database_id: DatabaseId,
        namespace_id: NamespaceId,
        previous_version: CatalogVersion,
        next_version: CatalogVersion,
        durable_lsn: Option<u64>,
        marker: Option<u64>,
        record_count: usize,
    }

    impl CatalogPublicationReceiptView for TestReceipt {
        type DurabilityMarker = u64;

        fn batch_id_value(&self) -> u64 {
            self.batch_id
        }

        fn database_id(&self) -> DatabaseId {
            self.database_id
        }

        fn namespace_id(&self) -> NamespaceId {
            self.namespace_id
        }

        fn previous_version(&self) -> CatalogVersion {
            self.previous_version
        }

        fn next_version(&self) -> CatalogVersion {
            self.next_version
        }

        fn durable_lsn(&self) -> Option<u64> {
            self.durable_lsn
        }

        fn durable_evidence_marker(&self) -> Option<&Self::DurabilityMarker> {
            self.marker.as_ref()
        }

        fn record_count(&self) -> usize {
            self.record_count
        }

        fn source_hash_is_zero(&self) -> bool {
            false
        }

        fn dependency_graph_hash_is_zero(&self) -> bool {
            false
        }

        fn publication_semantics(&self) -> CatalogPublicationSemantics {
            CatalogPublicationSemantics::DurablePublicationExternal
        }
    }

    #[test]
    fn generic_publication_registry_restores_hadr_progress_without_catalog_runtime() {
        let publication = test_publication(1, 2);
        let terminal = CatalogPublicationReplayTerminalRecord {
            key: CatalogPublicationReplayKey::from_receipt(&publication.receipt),
            outcome: CatalogPublicationReplayTerminalOutcome::Committed,
            durable_lsn: publication.receipt.durable_lsn(),
            durable_evidence_marker: publication.receipt.durable_evidence_marker().copied(),
            record_count: publication.receipt.record_count(),
            audit_trace_id: publication.audit_trace.trace_id.clone(),
        };
        let audit_evidence =
            catalog_visible_change_audit_evidence_for_publication(&publication, 1, 2);
        let subscriber_id = "hadr-a".to_string();
        let acknowledgement = CatalogSubscriptionAcknowledgement {
            subscriber_id: subscriber_id.clone(),
            database_id: publication.receipt.database_id(),
            namespace_id: publication.receipt.namespace_id(),
            acknowledged_version: publication.receipt.next_version(),
            durable_lsn_seen: publication.receipt.durable_lsn(),
            durable_evidence_marker_seen: publication.receipt.durable_evidence_marker().copied(),
            replayed_record_count: publication.receipt.record_count(),
            audit_trace_id: publication.audit_trace.trace_id.clone(),
        };
        let records = vec![
            CatalogPublicationSubscriptionReplayRecord::Terminal(terminal),
            CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
                publication,
                audit_evidence,
            },
            CatalogPublicationSubscriptionReplayRecord::SubscriptionAcknowledgement(
                acknowledgement,
            ),
        ];

        let registry =
            CatalogPublicationSubscriberRegistry::<TestReceipt, String>::restore_from_replay(
                records,
                vec![CatalogSubscriberRegistration::hadr_replica(
                    subscriber_id.clone(),
                )],
            )
            .expect("generic registry should restore replay records");

        let progress = registry
            .subscriber_progress(&subscriber_id)
            .expect("subscriber progress restored");
        assert_eq!(progress.acknowledged_version, version(2));
        let evidence = registry
            .hadr_replay_evidence(&subscriber_id)
            .expect("hadr evidence validation succeeds")
            .expect("hadr evidence restored");
        assert_eq!(evidence.restored_catalog_version, version(2));
        assert_eq!(evidence.durable_evidence_marker_seen, Some(44));
    }

    #[test]
    fn generic_publication_registry_rejects_unregistered_acknowledgement() {
        let publication = test_publication(1, 2);
        let terminal = CatalogPublicationReplayTerminalRecord {
            key: CatalogPublicationReplayKey::from_receipt(&publication.receipt),
            outcome: CatalogPublicationReplayTerminalOutcome::Committed,
            durable_lsn: publication.receipt.durable_lsn(),
            durable_evidence_marker: publication.receipt.durable_evidence_marker().copied(),
            record_count: publication.receipt.record_count(),
            audit_trace_id: publication.audit_trace.trace_id.clone(),
        };
        let audit_evidence =
            catalog_visible_change_audit_evidence_for_publication(&publication, 1, 2);
        let mut registry = CatalogPublicationSubscriberRegistry::<TestReceipt, String>::new();
        registry
            .record_visible_publication(publication.clone(), audit_evidence, terminal)
            .expect("publication is visible");
        let error = registry
            .acknowledge(CatalogSubscriptionAcknowledgement {
                subscriber_id: "missing".to_string(),
                database_id: publication.receipt.database_id(),
                namespace_id: publication.receipt.namespace_id(),
                acknowledged_version: publication.receipt.next_version(),
                durable_lsn_seen: publication.receipt.durable_lsn(),
                durable_evidence_marker_seen: publication
                    .receipt
                    .durable_evidence_marker()
                    .copied(),
                replayed_record_count: publication.receipt.record_count(),
                audit_trace_id: publication.audit_trace.trace_id.clone(),
            })
            .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("registered subscriber"));
    }

    fn test_publication(
        previous_version: u64,
        next_version: u64,
    ) -> CatalogPublicationReport<TestReceipt> {
        let receipt = TestReceipt {
            batch_id: 700,
            database_id: DatabaseId::new(10),
            namespace_id: NamespaceId::new(20),
            previous_version: version(previous_version),
            next_version: version(next_version),
            durable_lsn: Some(99),
            marker: Some(44),
            record_count: 3,
        };
        CatalogPublicationReport {
            audience: CatalogPublicationAudience::AdministrationHaOnly,
            receipt,
            published_objects: Vec::new(),
            plan_invalidation: CatalogPlanInvalidationReport {
                catalog_version: receipt.next_version,
                changed_contracts: Vec::new(),
            },
            recovery_replay: CatalogRecoveryReplayExpectation::from_receipt(&receipt),
            audit_trace: CatalogPublicationAuditTrace {
                trace_id: "trace-1".to_string(),
                operator_principal: "catalog-admin".to_string(),
                reason_code: CatalogPublicationReasonCode::DefinitionBatchCommitted,
            },
        }
    }

    fn version(value: u64) -> CatalogVersion {
        CatalogVersion::new(value)
    }
}
