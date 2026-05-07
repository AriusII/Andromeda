use andromeda_error::AndromedaResult;
use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};

use super::{
    CatalogPublicationReport, CatalogSubscriberId, catalog_publication_error, require_equal,
};
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
        require_equal(
            &self.acknowledged_version,
            &publication.receipt.next_version,
            "catalog subscription acknowledgement version must match publication",
        )?;
        require_equal(
            &self.durable_lsn_seen,
            &publication.receipt.durable_lsn,
            "catalog subscription acknowledgement durable LSN must match publication",
        )?;
        require_equal(
            &self.durable_evidence_marker_seen,
            &publication.receipt.durable_evidence_marker,
            "catalog subscription acknowledgement durable marker must match publication",
        )?;
        require_equal(
            &self.replayed_record_count,
            &publication.receipt.record_count,
            "catalog subscription acknowledgement replayed record count must match publication",
        )?;
        require_equal(
            &self.audit_trace_id,
            &publication.audit_trace.trace_id,
            "catalog subscription acknowledgement audit trace id must match publication",
        )?;
        Ok(())
    }
}
