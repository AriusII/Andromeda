use andromeda_catalog::{
    CatalogObjectRef, CatalogPlanInvalidationReport, CatalogPublicationAudience,
    CatalogPublicationAuditTrace, CatalogPublicationReasonCode, CatalogPublicationReceipt,
    CatalogPublicationReplayTerminalOutcome, CatalogPublicationReplayTerminalRecord,
    CatalogPublicationReport, CatalogPublicationSemantics, CatalogPublicationSubscriberRegistry,
    CatalogPublishedContract, CatalogPublishedObject, CatalogRecoveryReplayExpectation,
    CatalogSubscriberId, CatalogSubscriberKind, CatalogSubscriberRegistration,
    CatalogSubscriptionAcknowledgement, CatalogVisibleChangeAuditEvidence,
    DefinitionBatchDependencyGraphHash, DefinitionBatchId, DefinitionBatchSourceHash, ObjectKind,
    QualifiedName, replay_publication_subscription_changes,
};
use andromeda_error::AndromedaErrorKind;
use andromeda_types::{
    CatalogObjectId, CatalogVersion, ContractHash, DatabaseId, NamespaceId, ProcedureId,
};

const DATABASE_ID: DatabaseId = DatabaseId::new(10);
const NAMESPACE_ID: NamespaceId = NamespaceId::new(20);

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
    let contract = CatalogPublishedContract {
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

fn acknowledgement(publication: &CatalogPublicationReport) -> CatalogSubscriptionAcknowledgement {
    CatalogSubscriptionAcknowledgement {
        subscriber_id: CatalogSubscriberId::new("hadr-replica-a").unwrap(),
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
    publication: &CatalogPublicationReport,
) -> (
    CatalogPublicationReplayTerminalRecord,
    CatalogVisibleChangeAuditEvidence,
) {
    (
        CatalogPublicationReplayTerminalRecord::committed_for_publication(publication),
        CatalogVisibleChangeAuditEvidence::for_publication(publication, 1, 2),
    )
}

#[test]
fn replay_applies_visible_publication_only_after_prior_audit_evidence() {
    let publication = report();
    let (terminal, audit_evidence) = visible_records(&publication);

    let summary = replay_publication_subscription_changes([
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::Terminal(terminal),
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
            publication,
            audit_evidence,
        },
    ])
    .unwrap();

    assert_eq!(summary.applied_publication_count, 1);
    assert_eq!(summary.terminal_record_count, 1);
    assert_eq!(summary.audit_before_visible_change_count, 1);
    assert_eq!(
        summary.final_visible_catalog_version,
        Some(CatalogVersion::new(8))
    );
}

#[test]
fn replay_is_idempotent_for_duplicate_publication_ack_and_terminal_records() {
    let publication = report();
    let (_terminal, audit_evidence) = visible_records(&publication);
    let acknowledgement = acknowledgement(&publication);
    let terminal = CatalogPublicationReplayTerminalRecord::committed_for_publication(&publication);

    let summary = replay_publication_subscription_changes([
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::Terminal(terminal.clone()),
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::Terminal(terminal),
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
            publication: publication.clone(),
            audit_evidence: audit_evidence.clone(),
        },
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
            publication: publication.clone(),
            audit_evidence,
        },
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::SubscriptionAcknowledgement(
            acknowledgement.clone(),
        ),
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::SubscriptionAcknowledgement(
            acknowledgement,
        ),
    ])
    .unwrap();

    assert_eq!(summary.applied_publication_count, 1);
    assert_eq!(summary.acknowledged_subscription_count, 1);
    assert_eq!(summary.terminal_record_count, 1);
    assert_eq!(summary.duplicate_record_count, 3);
}

#[test]
fn restore_from_replay_reconstructs_durable_publication_state_and_hadr_progress() {
    let publication = report();
    let (terminal, audit_evidence) = visible_records(&publication);
    let acknowledgement = acknowledgement(&publication);
    let subscriber_id = acknowledgement.subscriber_id.clone();

    let registry = CatalogPublicationSubscriberRegistry::restore_from_replay(
        [
            andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::Terminal(terminal),
            andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
                publication: publication.clone(),
                audit_evidence,
            },
            andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::SubscriptionAcknowledgement(
                acknowledgement,
            ),
        ],
        [CatalogSubscriberRegistration::hadr_replica(
            subscriber_id.clone(),
        )],
    )
    .unwrap();

    let summary = registry.replay_summary().unwrap();
    assert_eq!(summary.applied_publication_count, 1);
    assert_eq!(summary.acknowledged_subscription_count, 1);
    assert_eq!(summary.terminal_record_count, 1);
    assert_eq!(
        summary.final_visible_catalog_version,
        Some(publication.receipt.next_version)
    );
    assert_eq!(registry.records().len(), 3);

    let progress = registry.subscriber_progress(&subscriber_id).unwrap();
    assert_eq!(progress.subscriber_kind, CatalogSubscriberKind::HadrReplica);
    assert_eq!(
        progress.acknowledged_version,
        publication.receipt.next_version
    );
    assert_eq!(
        progress.replayed_record_count,
        publication.receipt.record_count
    );
    assert_eq!(
        progress.expected_record_count,
        publication.receipt.record_count
    );
    assert_eq!(progress.durable_lsn_seen, publication.receipt.durable_lsn);
    assert_eq!(progress.audit_trace_id, publication.audit_trace.trace_id);

    let hadr_evidence = registry
        .hadr_replay_evidence(&subscriber_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        hadr_evidence.restored_catalog_version,
        publication.receipt.next_version
    );
    assert_eq!(
        hadr_evidence.replayed_record_count,
        publication.receipt.record_count
    );
    assert_eq!(
        hadr_evidence.expected_record_count,
        publication.receipt.record_count
    );
    assert_eq!(
        hadr_evidence.durable_lsn_seen,
        publication.receipt.durable_lsn
    );
    assert_eq!(
        hadr_evidence.audit_trace_id,
        publication.audit_trace.trace_id
    );
}

#[test]
fn replay_rejects_visible_publication_when_terminal_record_count_is_partial() {
    let publication = report();
    let (mut terminal, audit_evidence) = visible_records(&publication);
    terminal.record_count = publication.receipt.record_count - 1;

    let error = replay_publication_subscription_changes([
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::Terminal(terminal),
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
            publication,
            audit_evidence,
        },
    ])
    .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("record count"));
}

#[test]
fn replay_rejects_visible_publications_out_of_catalog_version_order() {
    let later_publication = report_for_versions(31, 8, 9);
    let earlier_publication = report_for_versions(30, 7, 8);
    let (later_terminal, later_audit_evidence) = visible_records(&later_publication);
    let (earlier_terminal, earlier_audit_evidence) = visible_records(&earlier_publication);

    let error = replay_publication_subscription_changes([
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::Terminal(later_terminal),
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
            publication: later_publication,
            audit_evidence: later_audit_evidence,
        },
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::Terminal(earlier_terminal),
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
            publication: earlier_publication,
            audit_evidence: earlier_audit_evidence,
        },
    ])
    .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("version ordering"));
}

#[test]
fn replay_rejects_visible_publication_that_skips_catalog_version() {
    let publication = report_for_versions(30, 7, 9);
    let (terminal, audit_evidence) = visible_records(&publication);

    let error = replay_publication_subscription_changes([
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::Terminal(terminal),
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
            publication,
            audit_evidence,
        },
    ])
    .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("exactly one catalog version"));
}

#[test]
fn replay_rejects_zero_durable_lsn_evidence() {
    let mut publication = report();
    publication.receipt.durable_lsn = Some(0);
    publication.recovery_replay =
        CatalogRecoveryReplayExpectation::from_receipt(&publication.receipt);
    let (terminal, audit_evidence) = visible_records(&publication);

    let error = replay_publication_subscription_changes([
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::Terminal(terminal),
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
            publication,
            audit_evidence,
        },
    ])
    .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("durable"));
}

#[test]
fn replay_rejects_terminal_audit_trace_mismatch_for_visible_publication() {
    let publication = report();
    let (mut terminal, audit_evidence) = visible_records(&publication);
    terminal.audit_trace_id = "different-audit-trace".to_string();

    let error = replay_publication_subscription_changes([
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::Terminal(terminal),
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
            publication,
            audit_evidence,
        },
    ])
    .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("audit trace id"));
}

#[test]
fn replay_fails_closed_on_conflicting_publication_terminal_record() {
    let publication = report();
    let terminal = CatalogPublicationReplayTerminalRecord::committed_for_publication(&publication);
    let mut conflicting_terminal = terminal.clone();
    conflicting_terminal.outcome = CatalogPublicationReplayTerminalOutcome::Aborted;

    let error = replay_publication_subscription_changes([
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::Terminal(terminal),
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::Terminal(
            conflicting_terminal,
        ),
    ])
    .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("conflicting"));
}

#[test]
fn replay_rejects_visible_publication_when_audit_was_not_recorded_first() {
    let publication = report();
    let audit_evidence = CatalogVisibleChangeAuditEvidence::for_publication(&publication, 3, 2);

    let error = replay_publication_subscription_changes([
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
            publication,
            audit_evidence,
        },
    ])
    .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("before the visible change"));
}

#[test]
fn replay_rejects_visible_publication_without_prior_durable_terminal_record() {
    let publication = report();
    let audit_evidence = CatalogVisibleChangeAuditEvidence::for_publication(&publication, 1, 2);

    let error = replay_publication_subscription_changes([
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
            publication,
            audit_evidence,
        },
    ])
    .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("terminal"));
}

#[test]
fn replay_rejects_subscription_acknowledgement_without_visible_publication() {
    let publication = report();
    let acknowledgement = acknowledgement(&publication);

    let error = replay_publication_subscription_changes([
        andromeda_catalog::CatalogPublicationSubscriptionReplayRecord::SubscriptionAcknowledgement(
            acknowledgement,
        ),
    ])
    .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("requires a visible publication"));
}
