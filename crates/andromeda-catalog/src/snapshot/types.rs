//! Snapshot-related compatibility facade.
//!
//! `andromeda-catalog-store` owns portable snapshot publication and lifecycle
//! state. `andromeda-catalog` keeps these aliases while it owns DefinitionBatch
//! planning and mutation application.

use crate::CatalogPublicationReceipt;

pub use andromeda_catalog_store::{
    CatalogObjectLifecycle, CatalogObjectLifecycleStatus, CatalogSnapshotApplyReport,
};

pub type CatalogSnapshotPublication =
    andromeda_catalog_store::CatalogSnapshotPublication<CatalogPublicationReceipt>;
