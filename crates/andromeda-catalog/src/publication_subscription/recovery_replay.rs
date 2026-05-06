use andromeda_core::{AndromedaResult, CatalogVersion};

use super::catalog_publication_error;
use crate::CatalogPublicationReceipt;

/// Recovery/replay expectations attached to a durable publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogRecoveryReplayExpectation {
    pub starting_version: CatalogVersion,
    pub target_version: CatalogVersion,
    pub required_record_count: usize,
    pub require_exact_commit_boundary: bool,
}

impl CatalogRecoveryReplayExpectation {
    pub const fn from_receipt(receipt: &CatalogPublicationReceipt) -> Self {
        Self {
            starting_version: receipt.previous_version,
            target_version: receipt.next_version,
            required_record_count: receipt.record_count,
            require_exact_commit_boundary: true,
        }
    }

    pub fn validate_for_receipt(&self, receipt: &CatalogPublicationReceipt) -> AndromedaResult<()> {
        if self.starting_version != receipt.previous_version || self.target_version != receipt.next_version
        {
            return catalog_publication_error(
                "recovery replay expectation versions must match publication receipt",
            );
        }
        if self.required_record_count != receipt.record_count {
            return catalog_publication_error(
                "recovery replay expectation record count must match publication receipt",
            );
        }
        if !self.require_exact_commit_boundary {
            return catalog_publication_error(
                "catalog recovery replay must require exact commit boundary matching",
            );
        }
        Ok(())
    }
}
