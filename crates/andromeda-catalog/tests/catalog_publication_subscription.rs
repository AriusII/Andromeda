use andromeda_catalog::{
    CatalogDurabilityMarker, CatalogObjectRef, CatalogPlanInvalidationReport,
    CatalogPublicationAudience, CatalogPublicationAuditTrace, CatalogPublicationReasonCode,
    CatalogPublicationReceipt, CatalogPublicationReport, CatalogPublicationSemantics,
    CatalogPublishedContract, CatalogPublishedObject, CatalogRecoveryReplayExpectation,
    CatalogSubscriberId, CatalogSubscriptionAcknowledgement, DefinitionBatchId, ObjectKind,
    QualifiedName,
};
use andromeda_core::{
    AndromedaErrorKind, CatalogObjectId, CatalogVersion, ContractHash, DatabaseId, NamespaceId,
    ProcedureId,
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
        durable_lsn: Some(80),
        durable_evidence_marker: None,
        record_count: 3,
        publication_semantics: CatalogPublicationSemantics::DurablePublicationExternal,
    }
}

fn procedure_object() -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(99),
        name: QualifiedName::parse("Inventory.ReserveStock").unwrap(),
        kind: ObjectKind::Procedure,
        catalog_version: CatalogVersion::new(8),
    }
}

fn report() -> CatalogPublicationReport {
    let receipt = receipt();
    let contract = CatalogPublishedContract {
        procedure_id: ProcedureId::new(99),
        object: procedure_object(),
        contract_hash: ContractHash::test_vector(0xA5),
    };

    CatalogPublicationReport {
        audience: CatalogPublicationAudience::AdministrationHaOnly,
        receipt,
        published_objects: vec![CatalogPublishedObject {
            object: procedure_object(),
        }],
        plan_invalidation: CatalogPlanInvalidationReport {
            catalog_version: receipt.next_version,
            changed_contracts: vec![contract],
        },
        recovery_replay: CatalogRecoveryReplayExpectation::from_receipt(&receipt),
        audit_trace: CatalogPublicationAuditTrace {
            trace_id: "catalog-pub-8".to_string(),
            operator_principal: "system.catalog-admin".to_string(),
            reason_code: CatalogPublicationReasonCode::DefinitionBatchCommitted,
        },
    }
}

#[test]
fn publication_report_validates_admin_ha_contract_and_invalidation_identity() {
    let report = report();

    report.validate().unwrap();
    assert_eq!(
        report.plan_invalidation.catalog_version,
        CatalogVersion::new(8)
    );
    assert_eq!(
        report.recovery_replay,
        CatalogRecoveryReplayExpectation {
            starting_version: CatalogVersion::new(7),
            target_version: CatalogVersion::new(8),
            required_record_count: 3,
            require_exact_commit_boundary: true,
        }
    );
}

#[test]
fn publication_report_rejects_non_durable_or_stale_invalidation_evidence() {
    let mut non_durable = report();
    non_durable.receipt.durable_lsn = None;
    non_durable.receipt.durable_evidence_marker = None;

    let error = non_durable.validate().unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("durable"));

    let mut stale_invalidation = report();
    stale_invalidation.plan_invalidation.catalog_version = CatalogVersion::new(7);

    let error = stale_invalidation.validate().unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("plan invalidation"));
}

#[test]
fn subscription_acknowledgement_must_match_publication_replay_boundary() {
    let report = report();
    let ack = CatalogSubscriptionAcknowledgement {
        subscriber_id: CatalogSubscriberId::new("hadr-replica-a").unwrap(),
        database_id: DATABASE_ID,
        namespace_id: NAMESPACE_ID,
        acknowledged_version: CatalogVersion::new(8),
        durable_lsn_seen: Some(80),
        durable_evidence_marker_seen: None,
        replayed_record_count: 3,
        audit_trace_id: "catalog-pub-8".to_string(),
    };

    ack.validate_for_publication(&report).unwrap();

    let mut stale_ack = ack;
    stale_ack.acknowledged_version = CatalogVersion::new(7);
    let error = stale_ack.validate_for_publication(&report).unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("acknowledgement version"));
}

#[test]
fn subscription_acknowledgement_can_track_external_marker_publications() {
    let mut report = report();
    let marker = CatalogDurabilityMarker::new(44);
    report.receipt.durable_lsn = None;
    report.receipt.durable_evidence_marker = Some(marker);

    let ack = CatalogSubscriptionAcknowledgement {
        subscriber_id: CatalogSubscriberId::new("hadr-replica-b").unwrap(),
        database_id: DATABASE_ID,
        namespace_id: NAMESPACE_ID,
        acknowledged_version: report.receipt.next_version,
        durable_lsn_seen: None,
        durable_evidence_marker_seen: Some(marker),
        replayed_record_count: report.receipt.record_count,
        audit_trace_id: report.audit_trace.trace_id.clone(),
    };

    ack.validate_for_publication(&report).unwrap();
    assert_eq!(ack.subscriber_id.as_str(), "hadr-replica-b");
}
