//! Catalog WAL replay and recovery logic for storage-facing catalog records.
//!
//! This module owns deterministic replay of committed catalog WAL records into
//! a dependency-light snapshot projection. Storage may re-export this API, but
//! the replay runtime and contracts live here.

mod runner;
mod snapshot;
mod state;

pub use runner::{
    CatalogStorageReplayFromLsnReport, CatalogStorageReplayLsnRecord, replay_catalog_from_lsn,
    replay_catalog_wal_records,
};
pub use snapshot::CatalogSnapshot;

#[cfg(test)]
mod tests;
