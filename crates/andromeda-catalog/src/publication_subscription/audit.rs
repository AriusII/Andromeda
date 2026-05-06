use andromeda_core::AndromedaResult;

use super::{catalog_publication_error, CatalogPublicationReasonCode};

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
            return catalog_publication_error("catalog publication audit trace id must not be empty");
        }
        if self.operator_principal.trim().is_empty() {
            return catalog_publication_error(
                "catalog publication audit operator principal must not be empty",
            );
        }
        Ok(())
    }
}
