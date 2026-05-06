use andromeda_core::AndromedaResult;

use super::{
    CatalogPublicationReasonCode, CatalogPublicationReport, catalog_publication_error,
    require_equal, validate_receipt,
};
use crate::{CatalogDurabilityMarker, CatalogPublicationReceipt};

/// Minimal audit fields required to correlate a catalog publication decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublicationAuditTrace {
    pub trace_id: String,
    pub operator_principal: String,
    pub reason_code: CatalogPublicationReasonCode,
}

impl CatalogPublicationAuditTrace {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.trace_id.trim().is_empty() {
            return catalog_publication_error(
                "catalog publication audit trace id must not be empty",
            );
        }
        if self.operator_principal.trim().is_empty() {
            return catalog_publication_error(
                "catalog publication audit operator principal must not be empty",
            );
        }
        Ok(())
    }
}

/// Evidence that the audit record for a visible catalog publication was durably
/// accepted before the publication became visible to catalog subscribers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogVisibleChangeAuditEvidence {
    pub trace: CatalogPublicationAuditTrace,
    pub durable_lsn: Option<u64>,
    pub durable_evidence_marker: Option<CatalogDurabilityMarker>,
    pub audit_record_ordinal: u64,
    pub visible_change_record_ordinal: u64,
}

impl CatalogVisibleChangeAuditEvidence {
    pub fn for_publication(
        publication: &CatalogPublicationReport,
        audit_record_ordinal: u64,
        visible_change_record_ordinal: u64,
    ) -> Self {
        Self {
            trace: publication.audit_trace.clone(),
            durable_lsn: publication.receipt.durable_lsn,
            durable_evidence_marker: publication.receipt.durable_evidence_marker,
            audit_record_ordinal,
            visible_change_record_ordinal,
        }
    }

    pub fn validate_for_publication(
        &self,
        publication: &CatalogPublicationReport,
    ) -> AndromedaResult<()> {
        publication.validate()?;
        require_equal(
            &self.trace,
            &publication.audit_trace,
            "catalog visible change audit evidence must match publication audit trace",
        )?;
        self.validate_for_receipt(&publication.receipt)
    }

    pub fn validate_for_receipt(&self, receipt: &CatalogPublicationReceipt) -> AndromedaResult<()> {
        validate_receipt(receipt)?;
        self.trace.validate()?;
        require_equal(
            &self.durable_lsn,
            &receipt.durable_lsn,
            "catalog visible change audit durable LSN must match publication receipt",
        )?;
        require_equal(
            &self.durable_evidence_marker,
            &receipt.durable_evidence_marker,
            "catalog visible change audit durable marker must match publication receipt",
        )?;
        if self.audit_record_ordinal == 0 || self.visible_change_record_ordinal == 0 {
            return catalog_publication_error(
                "catalog visible change audit ordinals must not be zero",
            );
        }
        if self.audit_record_ordinal >= self.visible_change_record_ordinal {
            return catalog_publication_error(
                "catalog visible change audit evidence must be recorded before the visible change",
            );
        }
        Ok(())
    }
}
