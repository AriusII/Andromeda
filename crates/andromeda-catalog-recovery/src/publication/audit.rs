use andromeda_error::AndromedaResult;

use super::{
    CatalogPublicationReasonCode, CatalogPublicationReceiptView, CatalogPublicationReport,
    catalog_recovery_publication_error, require_equal,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublicationAuditTrace {
    pub trace_id: String,
    pub operator_principal: String,
    pub reason_code: CatalogPublicationReasonCode,
}

impl CatalogPublicationAuditTrace {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.trace_id.trim().is_empty() {
            return catalog_recovery_publication_error(
                "catalog publication audit trace id must not be empty",
            );
        }
        if self.operator_principal.trim().is_empty() {
            return catalog_recovery_publication_error(
                "catalog publication audit operator principal must not be empty",
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogVisibleChangeAuditEvidence<TDurabilityMarker> {
    pub trace: CatalogPublicationAuditTrace,
    pub durable_lsn: Option<u64>,
    pub durable_evidence_marker: Option<TDurabilityMarker>,
    pub audit_record_ordinal: u64,
    pub visible_change_record_ordinal: u64,
}

impl<TDurabilityMarker> CatalogVisibleChangeAuditEvidence<TDurabilityMarker>
where
    TDurabilityMarker: PartialEq,
{
    pub fn validate_against_expectation(
        &self,
        expected_trace: &CatalogPublicationAuditTrace,
        expected_durable_lsn: Option<u64>,
        expected_durable_evidence_marker: Option<&TDurabilityMarker>,
    ) -> AndromedaResult<()> {
        self.trace.validate()?;
        require_equal(
            &self.trace,
            expected_trace,
            "catalog visible change audit evidence must match publication audit trace",
        )?;
        require_equal(
            &self.durable_lsn,
            &expected_durable_lsn,
            "catalog visible change audit durable LSN must match publication receipt",
        )?;
        require_equal(
            &self.durable_evidence_marker.as_ref(),
            &expected_durable_evidence_marker,
            "catalog visible change audit durable marker must match publication receipt",
        )?;
        if self.audit_record_ordinal == 0 || self.visible_change_record_ordinal == 0 {
            return catalog_recovery_publication_error(
                "catalog visible change audit ordinals must not be zero",
            );
        }
        if self.audit_record_ordinal >= self.visible_change_record_ordinal {
            return catalog_recovery_publication_error(
                "catalog visible change audit evidence must be recorded before the visible change",
            );
        }
        Ok(())
    }
}

pub fn catalog_visible_change_audit_evidence_for_publication<TReceipt>(
    publication: &CatalogPublicationReport<TReceipt>,
    audit_record_ordinal: u64,
    visible_change_record_ordinal: u64,
) -> CatalogVisibleChangeAuditEvidence<TReceipt::DurabilityMarker>
where
    TReceipt: CatalogPublicationReceiptView,
{
    let durable_evidence = publication.durable_evidence();
    CatalogVisibleChangeAuditEvidence {
        trace: publication.audit_trace.clone(),
        durable_lsn: durable_evidence.durable_lsn,
        durable_evidence_marker: durable_evidence.durable_evidence_marker,
        audit_record_ordinal,
        visible_change_record_ordinal,
    }
}

pub fn validate_visible_change_audit_for_publication<TReceipt>(
    evidence: &CatalogVisibleChangeAuditEvidence<TReceipt::DurabilityMarker>,
    publication: &CatalogPublicationReport<TReceipt>,
) -> AndromedaResult<()>
where
    TReceipt: CatalogPublicationReceiptView,
{
    publication.validate()?;
    let durable_evidence = publication.durable_evidence();
    evidence.validate_against_expectation(
        &publication.audit_trace,
        durable_evidence.durable_lsn,
        durable_evidence.durable_evidence_marker.as_ref(),
    )
}
