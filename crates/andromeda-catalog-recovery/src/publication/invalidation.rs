use std::collections::BTreeSet;

use andromeda_catalog_store::{CatalogObjectRef, ObjectKind};
use andromeda_error::AndromedaResult;
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

use super::catalog_recovery_publication_error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPlanInvalidatedContract {
    pub procedure_id: ProcedureId,
    pub object: CatalogObjectRef,
    pub contract_hash: ContractHash,
}

impl CatalogPlanInvalidatedContract {
    pub fn validate_for_version(&self, visible_version: CatalogVersion) -> AndromedaResult<()> {
        if self.procedure_id.get() == 0 {
            return catalog_recovery_publication_error(
                "published contract procedure id must not be zero",
            );
        }
        self.object.validate_for_definition(ObjectKind::Procedure)?;
        if self.object.catalog_version > visible_version {
            return catalog_recovery_publication_error(
                "published contract object version must not exceed the publication version",
            );
        }
        if self.contract_hash.is_zero() {
            return catalog_recovery_publication_error("published contract hash must not be zero");
        }
        Ok(())
    }
}

/// Bounded invalidation report for plan-cache subscribers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPlanInvalidationReport {
    pub catalog_version: CatalogVersion,
    pub changed_contracts: Vec<CatalogPlanInvalidatedContract>,
}

impl CatalogPlanInvalidationReport {
    pub fn validate_for_visible_version(
        &self,
        visible_version: CatalogVersion,
    ) -> AndromedaResult<()> {
        if self.catalog_version != visible_version {
            return catalog_recovery_publication_error(
                "plan invalidation catalog version must match publication next version",
            );
        }

        let mut procedure_ids = BTreeSet::new();
        let mut object_ids = BTreeSet::new();
        for contract in &self.changed_contracts {
            contract.validate_for_version(visible_version)?;
            if !procedure_ids.insert(contract.procedure_id) {
                return catalog_recovery_publication_error(
                    "plan invalidation report must not duplicate procedure ids",
                );
            }
            if !object_ids.insert(contract.object.object_id) {
                return catalog_recovery_publication_error(
                    "plan invalidation report must not duplicate procedure object ids",
                );
            }
        }

        Ok(())
    }
}

pub type CatalogPublishedContract = CatalogPlanInvalidatedContract;
