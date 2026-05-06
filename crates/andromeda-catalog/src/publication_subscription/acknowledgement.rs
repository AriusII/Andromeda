use andromeda_core::{AndromedaResult, CatalogVersion, DatabaseId, NamespaceId};

use super::{catalog_publication_error, CatalogPublicationReport, CatalogSubscriberId};
use crate::CatalogDurabilityMarker;

/// A subscriber acknowledgement for a durable catalog publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSubscriptionAcknowledgement {
    pub subscriber_id: CatalogSubscriberId,
    pub database_id: DatabaseId,
    pub namespace_id: NamespaceId,
    pub acknowledged_version: CatalogVersion,
    pub durable_lsn_seen: Option<u64>,
    pub durable_evidence_marker_seen: Option<CatalogDurabilityMarker>,
    pub replayed_record_count: usize,
    pub audit_trace_id: String,
}

impl CatalogSubscriptionAcknowledgement {
    pub fn validate_for_publication(
        &self,
        publication: &CatalogPublicationReport,
    ) -> AndromedaResult<()> {
        publication.validate()?;
        if self.database_id != publication.receipt.database_id
            || self.namespace_id != publication.receipt.namespace_id
        {
            return catalog_publication_error(
                "catalog subscription acknowledgement identity must match publication",
            );
        }
        if self.acknowledged_version != publication.receipt.next_version {
            return catalog_publication_error(
                "catalog subscription acknowledgement version must match publication",
            );
        }
        if self.durable_lsn_seen != publication.receipt.durable_lsn {
            return catalog_publication_error(
                "catalog subscription acknowledgement durable LSN must match publication",
            );
        }
        if self.durable_evidence_marker_seen != publication.receipt.durable_evidence_marker {
            return catalog_publication_error(
                "catalog subscription acknowledgement durable marker must match publication",
            );
        }
        if self.replayed_record_count != publication.receipt.record_count {
            return catalog_publication_error(
                "catalog subscription acknowledgement replayed record count must match publication",
            );
        }
        if self.audit_trace_id != publication.audit_trace.trace_id {
            return catalog_publication_error(
                "catalog subscription acknowledgement audit trace id must match publication",
            );
        }
        Ok(())
    }
}
