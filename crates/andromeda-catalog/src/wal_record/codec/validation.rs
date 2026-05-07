//! Structural validation for durable catalog WAL payload records.

use andromeda_error::AndromedaResult;

use crate::{
    CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH, CatalogMutationBoundary, CatalogMutationDelta,
    CatalogMutationOperation, CatalogMutationRecord, CatalogPublicationSemantics,
};

use super::catalog_error;

pub(super) fn validate_record(record: &CatalogMutationRecord) -> AndromedaResult<()> {
    match record {
        CatalogMutationRecord::Begin(boundary) | CatalogMutationRecord::Commit(boundary) => {
            validate_boundary(boundary)
        }
        CatalogMutationRecord::Apply(delta) => validate_delta(delta),
    }
}

pub(super) fn validate_boundary(boundary: &CatalogMutationBoundary) -> AndromedaResult<()> {
    if boundary.batch_id.get() == 0 {
        return Err(catalog_error(
            "catalog WAL boundary batch id must not be zero",
        ));
    }
    if boundary.database_id.get() == 0 {
        return Err(catalog_error(
            "catalog WAL boundary database id must not be zero",
        ));
    }
    if boundary.namespace_id.get() == 0 {
        return Err(catalog_error(
            "catalog WAL boundary namespace id must not be zero",
        ));
    }
    if boundary.next_version.get() <= boundary.previous_version.get() {
        return Err(catalog_error(
            "catalog WAL boundary must advance the catalog version",
        ));
    }
    if boundary.source_hash.is_zero() {
        return Err(catalog_error(
            "catalog WAL boundary source hash must not be zero",
        ));
    }
    if boundary.dependency_graph_hash.is_zero() {
        return Err(catalog_error(
            "catalog WAL boundary dependency graph hash must not be zero",
        ));
    }
    if boundary.expected_apply_count == 0 {
        return Err(catalog_error(
            "catalog WAL boundary expected apply count must not be zero",
        ));
    }
    if boundary.expected_apply_count > CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH {
        return Err(catalog_error(&format!(
            "catalog WAL boundary expected apply count must not exceed {CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH}"
        )));
    }
    if boundary.publication_semantics != CatalogPublicationSemantics::DurablePublicationExternal {
        return Err(catalog_error(
            "catalog WAL boundary requires durable publication semantics",
        ));
    }
    Ok(())
}

pub(super) fn validate_delta(delta: &CatalogMutationDelta) -> AndromedaResult<()> {
    if delta.planned_version.get() == 0 {
        return Err(catalog_error(
            "catalog WAL delta planned version must not be zero",
        ));
    }
    match &delta.operation {
        CatalogMutationOperation::CreateObject { object, definition } => {
            definition.validate()?;
            if object != definition.object_ref() {
                return Err(catalog_error(
                    "catalog WAL create delta object must match its definition object",
                ));
            }
            if object.catalog_version != delta.planned_version {
                return Err(catalog_error(
                    "catalog WAL create delta version must match its object version",
                ));
            }
        }
        CatalogMutationOperation::DeprecateObject { target } => {
            target.validate()?;
        }
    }
    Ok(())
}
