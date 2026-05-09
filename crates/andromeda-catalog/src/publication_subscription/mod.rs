//! Catalog publication/subscription aliases for snapshot-owned catalog receipts.
//!
//! The generic publication, replay, and subscriber state machine lives in
//! `andromeda-catalog-recovery`. This module keeps only the concrete aliases
//! that bind those contracts to `CatalogPublicationReceipt` and
//! `CatalogDurabilityMarker`, plus the snapshot-receipt validation adapters.

use andromeda_error::AndromedaResult;

use crate::{CatalogDurabilityMarker, CatalogPublicationReceipt};

pub use andromeda_catalog_recovery::{
    CatalogPublicationAudience, CatalogPublicationReasonCode, CatalogPublicationReceiptExpectation,
    CatalogPublicationReplayKey, CatalogPublicationReplayTerminalOutcome,
    CatalogPublicationSubscriptionReplayEvidence, CatalogPublicationSubscriptionReplayRecordKind,
    CatalogRecoveryReplayExpectation, CatalogSubscriberId, CatalogSubscriberKind,
    CatalogSubscriberRegistration, CatalogSubscriptionReplayKey,
};

pub type CatalogPublicationAuditTrace = andromeda_catalog_recovery::CatalogPublicationAuditTrace;
pub type CatalogVisibleChangeAuditEvidence =
    andromeda_catalog_recovery::CatalogVisibleChangeAuditEvidence<CatalogDurabilityMarker>;
pub type CatalogSubscriptionAcknowledgement =
    andromeda_catalog_recovery::CatalogSubscriptionAcknowledgement<
        CatalogSubscriberId,
        CatalogDurabilityMarker,
    >;
pub type CatalogPlanInvalidatedContract =
    andromeda_catalog_recovery::CatalogPlanInvalidatedContract;
pub type CatalogPlanInvalidationReport = andromeda_catalog_recovery::CatalogPlanInvalidationReport;
pub type CatalogPublishedObject = andromeda_catalog_recovery::CatalogPublishedObject;
pub type CatalogPublishedContract = CatalogPlanInvalidatedContract;
pub type CatalogPublicationReport =
    andromeda_catalog_recovery::CatalogPublicationReport<CatalogPublicationReceipt>;
pub type CatalogPublicationReplayTerminalRecord =
    andromeda_catalog_recovery::CatalogPublicationReplayTerminalRecord<CatalogDurabilityMarker>;
pub type CatalogPublicationSubscriptionReplayRecord =
    andromeda_catalog_recovery::CatalogPublicationSubscriptionReplayRecord<
        CatalogPublicationReceipt,
        CatalogSubscriberId,
    >;
pub type CatalogPublicationSubscriptionReplaySummary =
    andromeda_catalog_recovery::CatalogPublicationSubscriptionReplaySummary;
pub type CatalogPublicationRuntimeState =
    andromeda_catalog_recovery::CatalogPublicationRuntimeState<CatalogDurabilityMarker>;
pub type CatalogSubscriberAckProgress = andromeda_catalog_recovery::CatalogSubscriberAckProgress<
    CatalogSubscriberId,
    CatalogDurabilityMarker,
>;
pub type CatalogHadrSubscriberReplayEvidence =
    andromeda_catalog_recovery::CatalogHadrSubscriberReplayEvidence<
        CatalogSubscriberId,
        CatalogDurabilityMarker,
    >;
pub type CatalogPublicationSubscriberRegistry =
    andromeda_catalog_recovery::CatalogPublicationSubscriberRegistry<
        CatalogPublicationReceipt,
        CatalogSubscriberId,
    >;

pub fn validate_subscription_acknowledgement_for_publication(
    acknowledgement: &CatalogSubscriptionAcknowledgement,
    publication: &CatalogPublicationReport,
) -> AndromedaResult<()> {
    publication.validate()?;
    acknowledgement.validate_against_expectation(&CatalogPublicationReceiptExpectation {
        database_id: publication.receipt.database_id,
        namespace_id: publication.receipt.namespace_id,
        next_version: publication.receipt.next_version,
        durable_lsn: publication.receipt.durable_lsn,
        durable_evidence_marker: publication.receipt.durable_evidence_marker.as_ref(),
        record_count: publication.receipt.record_count,
        audit_trace_id: publication.audit_trace.trace_id.as_str(),
    })
}

pub fn catalog_visible_change_audit_evidence_for_publication(
    publication: &CatalogPublicationReport,
    audit_record_ordinal: u64,
    visible_change_record_ordinal: u64,
) -> CatalogVisibleChangeAuditEvidence {
    CatalogVisibleChangeAuditEvidence {
        trace: publication.audit_trace.clone(),
        durable_lsn: publication.receipt.durable_lsn,
        durable_evidence_marker: publication.receipt.durable_evidence_marker,
        audit_record_ordinal,
        visible_change_record_ordinal,
    }
}

pub fn validate_visible_change_audit_for_publication(
    evidence: &CatalogVisibleChangeAuditEvidence,
    publication: &CatalogPublicationReport,
) -> AndromedaResult<()> {
    publication.validate()?;
    evidence.validate_against_expectation(
        &publication.audit_trace,
        publication.receipt.durable_lsn,
        publication.receipt.durable_evidence_marker.as_ref(),
    )
}

pub fn validate_visible_change_audit_for_receipt(
    evidence: &CatalogVisibleChangeAuditEvidence,
    receipt: &CatalogPublicationReceipt,
    audit_trace: &CatalogPublicationAuditTrace,
) -> AndromedaResult<()> {
    andromeda_catalog_recovery::validate_catalog_publication_receipt(receipt)?;
    evidence.validate_against_expectation(
        audit_trace,
        receipt.durable_lsn,
        receipt.durable_evidence_marker.as_ref(),
    )
}

pub fn validate_plan_invalidation_for_receipt(
    report: &CatalogPlanInvalidationReport,
    receipt: &CatalogPublicationReceipt,
) -> AndromedaResult<()> {
    report.validate_for_visible_version(receipt.next_version)
}

pub fn replay_publication_subscription_changes(
    records: impl IntoIterator<Item = CatalogPublicationSubscriptionReplayRecord>,
) -> AndromedaResult<CatalogPublicationSubscriptionReplaySummary> {
    andromeda_catalog_recovery::replay_publication_subscription_changes(records)
}
