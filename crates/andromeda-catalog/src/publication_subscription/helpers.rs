use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{CatalogPublicationReceipt, CatalogPublicationSemantics};

pub(super) fn validate_receipt(receipt: &CatalogPublicationReceipt) -> AndromedaResult<()> {
    if receipt.batch_id.get() == 0
        || receipt.database_id.get() == 0
        || receipt.namespace_id.get() == 0
    {
        return catalog_publication_error(
            "catalog publication receipt identity fields must not be zero",
        );
    }
    if receipt.previous_version.get() == 0 || receipt.next_version <= receipt.previous_version {
        return catalog_publication_error(
            "catalog publication receipt must advance a nonzero catalog version",
        );
    }
    let expected_next = receipt
        .previous_version
        .get()
        .checked_add(1)
        .ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog publication receipt version advance overflowed",
            )
        })?;
    if receipt.next_version.get() != expected_next {
        return catalog_publication_error(
            "catalog publication receipt must advance by exactly one catalog version",
        );
    }
    if receipt.record_count == 0 {
        return catalog_publication_error(
            "catalog publication receipt record count must not be zero",
        );
    }
    if receipt.source_hash.is_zero() || receipt.dependency_graph_hash.is_zero() {
        return catalog_publication_error(
            "catalog publication receipt DefinitionBatch hashes must not be zero",
        );
    }
    if receipt.publication_semantics != CatalogPublicationSemantics::DurablePublicationExternal {
        return catalog_publication_error(
            "catalog publication report requires durable publication semantics",
        );
    }
    if receipt.durable_lsn == Some(0) {
        return catalog_publication_error(
            "catalog publication receipt durable WAL LSN must not be zero",
        );
    }
    if receipt.durable_lsn.is_none() && receipt.durable_evidence_marker.is_none() {
        return catalog_publication_error(
            "catalog publication receipt must carry durable WAL LSN or durable marker evidence",
        );
    }
    Ok(())
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
