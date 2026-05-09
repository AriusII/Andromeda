//! Catalog compatibility facade for recovery-owned WAL record contracts.

mod codec;

pub use andromeda_catalog_recovery::{
    AlterCompatibilityPolicy, CATALOG_CHANGE_APPLY_WAL_KIND_TAG, CATALOG_CHANGE_BEGIN_WAL_KIND_TAG,
    CATALOG_CHANGE_COMMIT_WAL_KIND_TAG, CatalogWalRecord, CatalogWalRecordDesign,
    DropFailureReason,
};
