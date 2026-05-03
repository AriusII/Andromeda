//! Snapshot and manifest publication contracts.
//!
//! This facade keeps publication concepts together without changing the
//! existing root-level public API.

pub use crate::{
    ColdSegmentPublicationPlan, DatabaseManifest, DatabaseSnapshotPublication,
    SnapshotAvailabilityContract, SnapshotSegmentReference,
    validate_cold_segment_publication_boundary,
};
