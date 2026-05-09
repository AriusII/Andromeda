//! Catalog WAL replay and recovery logic.
//!
//! This module owns:
//! - `replay_catalog_wal_segment`: Replay all committed catalog WAL records to reconstruct in-memory catalog
//! - Version monotonicity validation
//! - Procedure ID existence validation
//! - Deterministic catalog rebuilding from WAL
//!
//! ## Recovery Semantics
//!
//! During startup, all committed catalog WAL records are replayed in order:
//! 1. Read all records from the WAL segment
//! 2. Filter to target catalog version
//! 3. Validate version monotonicity (no version reordering)
//! 4. Validate procedure ID references (all IDs in mutations must exist)
//! 5. Reconstruct in-memory catalog snapshot

mod lsn;
mod runner;
mod snapshot;
mod state;

pub use lsn::{CatalogReplayFromLsnReport, LsnBoundCatalogRecord};
pub use runner::{replay_catalog_from_lsn, replay_catalog_wal_records};
pub use snapshot::CatalogSnapshot;

#[cfg(test)]
mod tests;
