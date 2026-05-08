use andromeda_error::AndromedaResult;

use super::CatalogPublicationReport;
use crate::CatalogDurabilityMarker;

pub use andromeda_catalog_recovery::CatalogPublicationReceiptExpectation;

/// A subscriber acknowledgement for a durable catalog publication.
pub type CatalogSubscriptionAcknowledgement =
    andromeda_catalog_recovery::CatalogSubscriptionAcknowledgement<
        super::CatalogSubscriberId,
        CatalogDurabilityMarker,
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
