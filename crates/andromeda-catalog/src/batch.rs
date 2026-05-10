//! Definition batches for atomic multi-object catalog updates.

mod definition;
mod durability;
mod mutation;
mod plan;

pub use definition::CatalogDefinitionBatchPlanning;
pub use durability::{CatalogMutationCommitEvidence, CatalogPublicationReceipt};
pub use mutation::{
    CATALOG_MUTATION_MAX_APPLY_RECORDS_PER_BATCH, CatalogMutation, CatalogMutationBoundary,
    CatalogMutationDelta, CatalogMutationOperation, CatalogMutationPlan, CatalogMutationRecord,
    CatalogMutationRecordKind, CatalogWalPayloadDecodeError, CatalogWalPayloadDecodeErrorKind,
};
pub use plan::DefinitionBatchPlan;
