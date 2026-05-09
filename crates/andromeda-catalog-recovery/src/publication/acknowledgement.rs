use andromeda_error::AndromedaResult;
use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};

use super::{
    CatalogPublicationReceiptView, CatalogPublicationReport, catalog_recovery_publication_error,
    require_equal,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSubscriptionAcknowledgement<TSubscriberId, TDurabilityMarker> {
    pub subscriber_id: TSubscriberId,
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub acknowledged_version: CatalogVersion,
    pub durable_lsn_seen: Option<u64>,
    pub durable_evidence_marker_seen: Option<TDurabilityMarker>,
    pub replayed_record_count: usize,
    pub audit_trace_id: String,
}

/// Publication durability envelope expected by acknowledgement checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublicationReceiptExpectation<'a, TDurabilityMarker> {
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub next_version: CatalogVersion,
    pub durable_lsn: Option<u64>,
    pub durable_evidence_marker: Option<&'a TDurabilityMarker>,
    pub record_count: usize,
    pub audit_trace_id: &'a str,
}

impl<TSubscriberId, TDurabilityMarker>
    CatalogSubscriptionAcknowledgement<TSubscriberId, TDurabilityMarker>
where
    TDurabilityMarker: PartialEq,
{
    pub fn validate_against_expectation(
        &self,
        expected: &CatalogPublicationReceiptExpectation<'_, TDurabilityMarker>,
    ) -> AndromedaResult<()> {
        if self.database_id != expected.database_id || self.namespace_id != expected.namespace_id {
            return catalog_recovery_publication_error(
                "catalog subscription acknowledgement identity must match publication",
            );
        }
        require_equal(
            &self.acknowledged_version,
            &expected.next_version,
            "catalog subscription acknowledgement version must match publication",
        )?;
        require_equal(
            &self.durable_lsn_seen,
            &expected.durable_lsn,
            "catalog subscription acknowledgement durable LSN must match publication",
        )?;
        require_equal(
            &self.durable_evidence_marker_seen.as_ref(),
            &expected.durable_evidence_marker,
            "catalog subscription acknowledgement durable marker must match publication",
        )?;
        require_equal(
            &self.replayed_record_count,
            &expected.record_count,
            "catalog subscription acknowledgement replayed record count must match publication",
        )?;
        require_equal(
            self.audit_trace_id.as_str(),
            expected.audit_trace_id,
            "catalog subscription acknowledgement audit trace id must match publication",
        )
    }
}

pub fn validate_subscription_acknowledgement_for_publication<TReceipt, TSubscriberId>(
    acknowledgement: &CatalogSubscriptionAcknowledgement<TSubscriberId, TReceipt::DurabilityMarker>,
    publication: &CatalogPublicationReport<TReceipt>,
) -> AndromedaResult<()>
where
    TReceipt: CatalogPublicationReceiptView,
{
    publication.validate()?;
    acknowledgement.validate_against_expectation(&CatalogPublicationReceiptExpectation {
        database_id: publication.receipt.database_id(),
        namespace_id: publication.receipt.namespace_id(),
        next_version: publication.receipt.next_version(),
        durable_lsn: publication.receipt.durable_lsn(),
        durable_evidence_marker: publication.receipt.durable_evidence_marker(),
        record_count: publication.receipt.record_count(),
        audit_trace_id: publication.audit_trace.trace_id.as_str(),
    })
}
