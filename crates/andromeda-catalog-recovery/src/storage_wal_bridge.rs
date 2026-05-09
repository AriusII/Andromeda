//! Storage WAL bridge for legacy catalog replay records.
//!
//! This module owns the deterministic byte representation and publication
//! replay adapter for the storage-facing catalog WAL record family. Storage
//! remains responsible for passing concrete `WalRecord` values from its runtime
//! integration paths.

mod codec;
mod error;
mod format_evidence;
mod record_parsing;
mod replay_planner;

#[cfg(test)]
mod tests;

pub use codec::{decode_storage_catalog_record, encode_storage_catalog_record};
pub use replay_planner::{
    CatalogStorageWalDurablePublication, CatalogStorageWalPublicationRecord,
    CatalogStorageWalPublicationReplayReport, replay_storage_catalog_publications_from_wal,
};
