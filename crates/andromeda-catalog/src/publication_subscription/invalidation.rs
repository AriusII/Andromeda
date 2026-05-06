use std::collections::BTreeSet;

use andromeda_core::{AndromedaResult, CatalogVersion};

use super::{CatalogPublishedContract, catalog_publication_error};
use crate::CatalogPublicationReceipt;

/// Bounded invalidation report for plan-cache subscribers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPlanInvalidationReport {
    pub catalog_version: CatalogVersion,
    pub changed_contracts: Vec<CatalogPublishedContract>,
}

impl CatalogPlanInvalidationReport {
    pub fn validate_for_receipt(&self, receipt: &CatalogPublicationReceipt) -> AndromedaResult<()> {
        if self.catalog_version != receipt.next_version {
            return catalog_publication_error(
                "plan invalidation catalog version must match publication next version",
            );
        }

        let mut procedure_ids = BTreeSet::new();
        let mut object_ids = BTreeSet::new();
        for contract in &self.changed_contracts {
            contract.validate_for_version(receipt.next_version)?;
            if !procedure_ids.insert(contract.procedure_id) {
                return catalog_publication_error(
                    "plan invalidation report must not duplicate procedure ids",
                );
            }
            if !object_ids.insert(contract.object.object_id) {
                return catalog_publication_error(
                    "plan invalidation report must not duplicate procedure object ids",
                );
            }
        }

        Ok(())
    }
}
