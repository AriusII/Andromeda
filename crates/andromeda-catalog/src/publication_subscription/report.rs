use std::collections::BTreeSet;

use andromeda_core::AndromedaResult;

use super::{
    CatalogPlanInvalidationReport, CatalogPublicationAudience, CatalogPublicationAuditTrace,
    CatalogPublishedObject, CatalogRecoveryReplayExpectation, catalog_publication_error,
    validate_receipt,
};
use crate::CatalogPublicationReceipt;

/// Durable publication report consumed only by administrative/HA subscribers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublicationReport {
    pub audience: CatalogPublicationAudience,
    pub receipt: CatalogPublicationReceipt,
    pub published_objects: Vec<CatalogPublishedObject>,
    pub plan_invalidation: CatalogPlanInvalidationReport,
    pub recovery_replay: CatalogRecoveryReplayExpectation,
    pub audit_trace: CatalogPublicationAuditTrace,
}

impl CatalogPublicationReport {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.audience != CatalogPublicationAudience::AdministrationHaOnly {
            return catalog_publication_error(
                "catalog publication report audience must be Administration/HA only",
            );
        }
        validate_receipt(&self.receipt)?;
        self.plan_invalidation.validate_for_receipt(&self.receipt)?;
        self.recovery_replay.validate_for_receipt(&self.receipt)?;
        self.audit_trace.validate()?;

        let mut object_ids = BTreeSet::new();
        for published in &self.published_objects {
            published.validate_for_version(self.receipt.next_version)?;
            if !object_ids.insert(published.object.object_id) {
                return catalog_publication_error(
                    "catalog publication report must not duplicate published object ids",
                );
            }
        }

        Ok(())
    }
}
