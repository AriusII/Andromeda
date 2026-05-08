use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::CatalogPublicationReceipt;

pub(super) fn validate_receipt(receipt: &CatalogPublicationReceipt) -> AndromedaResult<()> {
    andromeda_catalog_recovery::validate_catalog_publication_receipt(receipt)
}

pub(super) fn require_equal<T: PartialEq>(
    observed: &T,
    expected: &T,
    message: &'static str,
) -> AndromedaResult<()> {
    if observed == expected {
        Ok(())
    } else {
        catalog_publication_error(message)
    }
}

pub(super) fn catalog_publication_error<T>(message: &'static str) -> AndromedaResult<T> {
    Err(AndromedaError::new(AndromedaErrorKind::Catalog, message))
}
