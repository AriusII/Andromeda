//! Snapshot and manifest publication contracts.
//!
//! This facade keeps publication concepts together without changing the
//! existing root-level public API.

pub use crate::{
    DatabaseManifest, DatabaseSnapshotPublication, SnapshotAvailabilityContract,
    SnapshotSegmentReference,
};
