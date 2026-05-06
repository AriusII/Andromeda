use andromeda_core::{AndromedaResult, CatalogVersion, ContractHash, ProcedureId};

use super::catalog_publication_error;
use crate::{CatalogObjectRef, ObjectKind};

/// Object/version identity published for an applied catalog change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublishedObject {
    pub object: CatalogObjectRef,
}

impl CatalogPublishedObject {
    pub fn validate_for_version(&self, visible_version: CatalogVersion) -> AndromedaResult<()> {
        self.object.validate_for_definition(self.object.kind)?;
        if self.object.catalog_version > visible_version {
            return catalog_publication_error(
                "published catalog object version must not exceed the publication version",
            );
        }
        Ok(())
    }
}

/// Procedure contract identity that participates in plan-cache invalidation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublishedContract {
    pub procedure_id: ProcedureId,
    pub object: CatalogObjectRef,
    pub contract_hash: ContractHash,
}

impl CatalogPublishedContract {
    pub fn validate_for_version(&self, visible_version: CatalogVersion) -> AndromedaResult<()> {
        if self.procedure_id.get() == 0 {
            return catalog_publication_error("published contract procedure id must not be zero");
        }
        self.object.validate_for_definition(ObjectKind::Procedure)?;
        if self.object.catalog_version > visible_version {
            return catalog_publication_error(
                "published contract object version must not exceed the publication version",
            );
        }
        if self.contract_hash.is_zero() {
            return catalog_publication_error("published contract hash must not be zero");
        }
        Ok(())
    }
}
