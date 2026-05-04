//! Snapshot and manifest publication contracts.
//!
//! This facade keeps publication concepts together without changing the
//! existing root-level public API.

pub use crate::{
    validate_cold_segment_publication_boundary, ColdSegmentPublicationPlan, DatabaseManifest,
    DatabaseSnapshotPublication, SnapshotAvailabilityContract, SnapshotSegmentReference,
};
