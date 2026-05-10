use andromeda_catalog_recovery::{
    CatalogPlanInvalidatedContract, CatalogPlanInvalidationReport, CatalogPublicationAudience,
    CatalogPublicationAuditTrace, CatalogPublicationReasonCode,
    CatalogPublicationReplayTerminalOutcome, CatalogPublishedObject,
    CatalogRecoveryReplayExpectation, CatalogSubscriberId, CatalogSubscriberRegistration,
    catalog_visible_change_audit_evidence_for_publication,
};
use andromeda_catalog_store::{
    CatalogDurabilityMarker, CatalogObjectRef, CatalogPublicationSemantics, ObjectKind,
    QualifiedName,
};
use andromeda_definition_batch::{
    DefinitionBatchDependencyGraphHash, DefinitionBatchId, DefinitionBatchSourceHash,
};
use andromeda_error::AndromedaErrorKind;
use andromeda_types::{
    CatalogObjectId, CatalogVersion, ContractHash, DatabaseId, NamespaceId, ProcedureId,
};

const DATABASE_ID: DatabaseId = DatabaseId::new(10);
const NAMESPACE_ID: NamespaceId = NamespaceId::new(20);

type CatalogPublicationReceipt = andromeda_catalog_store::CatalogPublicationReceipt<
    DefinitionBatchId,
    DefinitionBatchSourceHash,
    DefinitionBatchDependencyGraphHash,
>;
type CatalogPublicationReport =
    andromeda_catalog_recovery::CatalogPublicationReport<CatalogPublicationReceipt>;
type CatalogPublicationReplayTerminalRecord =
    andromeda_catalog_recovery::CatalogPublicationReplayTerminalRecord<CatalogDurabilityMarker>;
type CatalogPublicationSubscriptionReplayRecord =
    andromeda_catalog_recovery::CatalogPublicationSubscriptionReplayRecord<
        CatalogPublicationReceipt,
        CatalogSubscriberId,
    >;
type CatalogPublicationSubscriberRegistry =
    andromeda_catalog_recovery::CatalogPublicationSubscriberRegistry<
        CatalogPublicationReceipt,
        CatalogSubscriberId,
    >;
type CatalogSubscriptionAcknowledgement =
    andromeda_catalog_recovery::CatalogSubscriptionAcknowledgement<
        CatalogSubscriberId,
        CatalogDurabilityMarker,
    >;
type CatalogVisibleChangeAuditEvidence =
    andromeda_catalog_recovery::CatalogVisibleChangeAuditEvidence<CatalogDurabilityMarker>;

fn receipt() -> CatalogPublicationReceipt {
    CatalogPublicationReceipt {
        batch_id: DefinitionBatchId::new(30),
        database_id: DATABASE_ID,
        namespace_id: NAMESPACE_ID,
        previous_version: CatalogVersion::new(7),
        next_version: CatalogVersion::new(8),
        source_hash: DefinitionBatchSourceHash::new([0x11; 32]),
        dependency_graph_hash: DefinitionBatchDependencyGraphHash::new([0x22; 32]),
        durable_lsn: Some(80),
        durable_evidence_marker: None,
        record_count: 3,
        publication_semantics: CatalogPublicationSemantics::DurablePublicationExternal,
    }
}

fn procedure_object_at(catalog_version: CatalogVersion) -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(99),
        name: QualifiedName::parse("Inventory.ReserveStock").unwrap(),
        kind: ObjectKind::Procedure,
        catalog_version,
    }
}

fn report_for_versions(
    batch_id: u64,
    previous_version: u64,
    next_version: u64,
) -> CatalogPublicationReport {
    let mut receipt = receipt();
    receipt.batch_id = DefinitionBatchId::new(batch_id);
    receipt.previous_version = CatalogVersion::new(previous_version);
    receipt.next_version = CatalogVersion::new(next_version);
    let object = procedure_object_at(receipt.next_version);
    let contract = CatalogPlanInvalidatedContract {
        procedure_id: ProcedureId::new(99),
        object: object.clone(),
        contract_hash: ContractHash::test_vector(0xA5),
    };

    CatalogPublicationReport {
        audience: CatalogPublicationAudience::AdministrationHaOnly,
        receipt,
        published_objects: vec![CatalogPublishedObject { object }],
        plan_invalidation: CatalogPlanInvalidationReport {
            catalog_version: receipt.next_version,
            changed_contracts: vec![contract],
        },
        recovery_replay: CatalogRecoveryReplayExpectation::from_receipt(&receipt),
        audit_trace: CatalogPublicationAuditTrace {
            trace_id: format!("catalog-pub-{next_version}"),
            operator_principal: "system.catalog-admin".to_string(),
            reason_code: CatalogPublicationReasonCode::DefinitionBatchCommitted,
        },
    }
}

fn report() -> CatalogPublicationReport {
    report_for_versions(30, 7, 8)
}

fn acknowledgement(
    publication: &CatalogPublicationReport,
    subscriber_id: &CatalogSubscriberId,
) -> CatalogSubscriptionAcknowledgement {
    CatalogSubscriptionAcknowledgement {
        subscriber_id: subscriber_id.clone(),
        database_id: DATABASE_ID,
        namespace_id: NAMESPACE_ID,
        acknowledged_version: publication.receipt.next_version,
        durable_lsn_seen: publication.receipt.durable_lsn,
        durable_evidence_marker_seen: publication.receipt.durable_evidence_marker,
        replayed_record_count: publication.receipt.record_count,
        audit_trace_id: publication.audit_trace.trace_id.clone(),
    }
}

fn visible_records(
    publication: CatalogPublicationReport,
) -> (
    CatalogPublicationReplayTerminalRecord,
    CatalogVisibleChangeAuditEvidence,
) {
    let terminal = CatalogPublicationReplayTerminalRecord::committed_for_publication(&publication);
    let audit_evidence = catalog_visible_change_audit_evidence_for_publication(&publication, 1, 2);
    (terminal, audit_evidence)
}

fn registry_with_hadr_subscriber(
    subscriber_id: &CatalogSubscriberId,
) -> CatalogPublicationSubscriberRegistry {
    let mut registry = CatalogPublicationSubscriberRegistry::new();
    registry
        .register_subscriber(CatalogSubscriberRegistration::hadr_replica(
            subscriber_id.clone(),
        ))
        .unwrap();
    registry
}

#[test]
fn catalog_publication_requires_durable_lsn_or_marker() {
    let mut publication = report();
    publication.receipt.durable_lsn = None;
    publication.receipt.durable_evidence_marker = None;
    publication.recovery_replay =
        CatalogRecoveryReplayExpectation::from_receipt(&publication.receipt);
    let (terminal, audit_evidence) = visible_records(publication.clone());

    let mut registry = CatalogPublicationSubscriberRegistry::new();
    let error = registry
        .record_visible_publication(publication, audit_evidence, terminal)
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("durable"));

    let mut zero_lsn_publication = report();
    zero_lsn_publication.receipt.durable_lsn = Some(0);
    zero_lsn_publication.recovery_replay =
        CatalogRecoveryReplayExpectation::from_receipt(&zero_lsn_publication.receipt);
    let (terminal, audit_evidence) = visible_records(zero_lsn_publication.clone());

    let error = registry
        .record_visible_publication(zero_lsn_publication, audit_evidence, terminal)
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("durable"));
}

#[test]
fn catalog_publication_report_requires_administration_hadr_audience() {
    let mut publication = report();
    publication.audience = CatalogPublicationAudience::RuntimeSubscribers;
    let (terminal, audit_evidence) = visible_records(publication.clone());

    let mut registry = CatalogPublicationSubscriberRegistry::new();
    let error = registry
        .record_visible_publication(publication, audit_evidence, terminal)
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("Administration/HA"));
}

#[test]
fn catalog_publication_report_rejects_replay_and_invalidation_mismatch() {
    let mut stale_invalidation = report();
    stale_invalidation.plan_invalidation.catalog_version = CatalogVersion::new(7);
    let (terminal, audit_evidence) = visible_records(stale_invalidation.clone());

    let mut registry = CatalogPublicationSubscriberRegistry::new();
    let error = registry
        .record_visible_publication(stale_invalidation, audit_evidence, terminal)
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("plan invalidation"));

    let mut mismatched_replay = report();
    mismatched_replay.recovery_replay.target_version = CatalogVersion::new(9);
    let (terminal, audit_evidence) = visible_records(mismatched_replay.clone());

    let error = registry
        .record_visible_publication(mismatched_replay, audit_evidence, terminal)
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(
        error
            .message()
            .contains("recovery replay expectation target version")
    );
}

#[test]
fn runtime_registry_rejects_duplicate_visible_publication_evidence() {
    let publication = report();
    let (terminal, audit_evidence) = visible_records(publication.clone());
    let mut registry = CatalogPublicationSubscriberRegistry::new();
    registry
        .record_visible_publication(publication.clone(), audit_evidence, terminal)
        .unwrap();

    let (duplicate_terminal, duplicate_audit_evidence) = visible_records(publication.clone());
    let error = registry
        .record_visible_publication(publication, duplicate_audit_evidence, duplicate_terminal)
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("duplicate publication evidence"));
}

#[test]
fn subscriber_ack_must_match_version_and_record_count() {
    let publication = report();
    let subscriber_id = CatalogSubscriberId::new("hadr-replica-a").unwrap();
    let mut registry = registry_with_hadr_subscriber(&subscriber_id);
    let (terminal, audit_evidence) = visible_records(publication.clone());
    registry
        .record_visible_publication(publication.clone(), audit_evidence, terminal)
        .unwrap();

    let mut stale_version = acknowledgement(&publication, &subscriber_id);
    stale_version.acknowledged_version = CatalogVersion::new(7);
    let error = registry.acknowledge(stale_version).unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("version"));

    let mut wrong_record_count = acknowledgement(&publication, &subscriber_id);
    wrong_record_count.replayed_record_count = 2;
    let error = registry.acknowledge(wrong_record_count).unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("record count"));

    let progress = registry
        .acknowledge(acknowledgement(&publication, &subscriber_id))
        .unwrap();
    assert_eq!(progress.acknowledged_version, CatalogVersion::new(8));
    assert_eq!(progress.expected_version, CatalogVersion::new(8));
    assert_eq!(progress.replayed_record_count, 3);
    assert_eq!(progress.expected_record_count, 3);
}

#[test]
fn hadr_subscriber_replay_restores_catalog_version() {
    let publication = report();
    let subscriber_id = CatalogSubscriberId::new("hadr-replica-a").unwrap();
    let acknowledgement = acknowledgement(&publication, &subscriber_id);
    let (terminal, audit_evidence) = visible_records(publication.clone());

    let registry = CatalogPublicationSubscriberRegistry::restore_from_replay(
        [
            CatalogPublicationSubscriptionReplayRecord::Terminal(terminal),
            CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
                publication,
                audit_evidence,
            },
            CatalogPublicationSubscriptionReplayRecord::SubscriptionAcknowledgement(
                acknowledgement,
            ),
        ],
        [CatalogSubscriberRegistration::hadr_replica(
            subscriber_id.clone(),
        )],
    )
    .unwrap();

    let summary = registry.replay_summary().unwrap();
    assert_eq!(
        summary.final_visible_catalog_version,
        Some(CatalogVersion::new(8))
    );
    assert_eq!(summary.acknowledged_subscription_count, 1);

    let evidence = registry
        .hadr_replay_evidence(&subscriber_id)
        .unwrap()
        .unwrap();
    assert_eq!(evidence.restored_catalog_version, CatalogVersion::new(8));
    assert_eq!(evidence.replayed_record_count, 3);
    assert_eq!(evidence.expected_record_count, 3);
    assert_eq!(evidence.durable_lsn_seen, Some(80));
}

#[test]
fn hadr_subscriber_replay_detects_catalog_version_mismatch() {
    let publication = report();
    let subscriber_id = CatalogSubscriberId::new("hadr-replica-a").unwrap();
    let mut acknowledgement = acknowledgement(&publication, &subscriber_id);
    acknowledgement.acknowledged_version = CatalogVersion::new(9);
    let (terminal, audit_evidence) = visible_records(publication.clone());

    let error = CatalogPublicationSubscriberRegistry::restore_from_replay(
        [
            CatalogPublicationSubscriptionReplayRecord::Terminal(terminal),
            CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
                publication,
                audit_evidence,
            },
            CatalogPublicationSubscriptionReplayRecord::SubscriptionAcknowledgement(
                acknowledgement,
            ),
        ],
        [CatalogSubscriberRegistration::hadr_replica(
            subscriber_id.clone(),
        )],
    )
    .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("version"));
}

#[test]
fn runtime_registry_rejects_visible_publications_out_of_catalog_version_order() {
    let later_publication = report_for_versions(31, 8, 9);
    let earlier_publication = report_for_versions(30, 7, 8);
    let (later_terminal, later_audit_evidence) = visible_records(later_publication.clone());
    let (earlier_terminal, earlier_audit_evidence) = visible_records(earlier_publication.clone());

    let error = CatalogPublicationSubscriberRegistry::restore_from_replay(
        [
            CatalogPublicationSubscriptionReplayRecord::Terminal(later_terminal),
            CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
                publication: later_publication,
                audit_evidence: later_audit_evidence,
            },
            CatalogPublicationSubscriptionReplayRecord::Terminal(earlier_terminal),
            CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
                publication: earlier_publication,
                audit_evidence: earlier_audit_evidence,
            },
        ],
        [CatalogSubscriberRegistration::hadr_replica(
            CatalogSubscriberId::new("hadr-replica-a").unwrap(),
        )],
    )
    .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("version ordering"));
}

#[test]
fn runtime_registry_rejects_visible_publication_that_skips_catalog_version() {
    let skipped_publication = report_for_versions(30, 7, 9);
    let (terminal, audit_evidence) = visible_records(skipped_publication.clone());

    let error = CatalogPublicationSubscriberRegistry::restore_from_replay(
        [
            CatalogPublicationSubscriptionReplayRecord::Terminal(terminal),
            CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
                publication: skipped_publication,
                audit_evidence,
            },
        ],
        [CatalogSubscriberRegistration::hadr_replica(
            CatalogSubscriberId::new("hadr-replica-a").unwrap(),
        )],
    )
    .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("exactly one catalog version"));
}

#[test]
fn runtime_restore_fails_closed_on_terminal_conflict() {
    let publication = report();
    let terminal = CatalogPublicationReplayTerminalRecord::committed_for_publication(&publication);
    let mut conflicting_terminal = terminal.clone();
    conflicting_terminal.outcome = CatalogPublicationReplayTerminalOutcome::Aborted;

    let error = CatalogPublicationSubscriberRegistry::restore_from_replay(
        [
            CatalogPublicationSubscriptionReplayRecord::Terminal(terminal),
            CatalogPublicationSubscriptionReplayRecord::Terminal(conflicting_terminal),
        ],
        [CatalogSubscriberRegistration::hadr_replica(
            CatalogSubscriberId::new("hadr-replica-a").unwrap(),
        )],
    )
    .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("conflicting"));
}
