use crate::{CatalogMutationRecord, DefinitionBatchId};

pub(super) fn boundary_batch_id(record: &CatalogMutationRecord) -> Option<DefinitionBatchId> {
    match record {
        CatalogMutationRecord::Begin(boundary) | CatalogMutationRecord::Commit(boundary) => {
            Some(boundary.batch_id)
        },
        CatalogMutationRecord::Apply(_) => None,
    }
}
