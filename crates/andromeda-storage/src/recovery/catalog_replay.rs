//! Storage compatibility exports for catalog WAL replay.
//!
//! `andromeda-catalog-recovery` owns the runtime and contracts. Storage keeps
//! these historical names as a minimal adapter for existing callers.

pub use andromeda_catalog_recovery::{
    CatalogSnapshot, replay_catalog_from_lsn, replay_catalog_wal_records,
};

pub type LsnBoundCatalogRecord = andromeda_catalog_recovery::CatalogStorageReplayLsnRecord;
pub type CatalogReplayFromLsnReport = andromeda_catalog_recovery::CatalogStorageReplayFromLsnReport;
