use andromeda_error::AndromedaResult;

use super::{CatalogPublicationReport, validate_receipt};
use crate::{CatalogDurabilityMarker, CatalogPublicationReceipt};

pub type CatalogPublicationAuditTrace = andromeda_catalog_recovery::CatalogPublicationAuditTrace;
pub type CatalogVisibleChangeAuditEvidence =
    andromeda_catalog_recovery::CatalogVisibleChangeAuditEvidence<CatalogDurabilityMarker>;

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
    validate_receipt(receipt)?;
    evidence.validate_against_expectation(
        audit_trace,
        receipt.durable_lsn,
        receipt.durable_evidence_marker.as_ref(),
    )
}
