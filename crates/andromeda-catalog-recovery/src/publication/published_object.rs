use andromeda_catalog_store::CatalogObjectRef;
use andromeda_error::AndromedaResult;
use andromeda_types::CatalogVersion;

use super::catalog_recovery_publication_error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublishedObject {
    pub object: CatalogObjectRef,
}

impl CatalogPublishedObject {
    pub fn validate_for_version(&self, visible_version: CatalogVersion) -> AndromedaResult<()> {
        self.object.validate_for_definition(self.object.kind)?;
        if self.object.catalog_version > visible_version {
            return catalog_recovery_publication_error(
                "published catalog object version must not exceed the publication version",
            );
        }
        Ok(())
    }
}
