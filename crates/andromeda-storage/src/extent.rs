//! Compatibility re-exports for segment extent ownership.
//!
//! Extent descriptors and extent-manager contracts now live in
//! `andromeda-segment`. `andromeda-storage` keeps this module as the legacy
//! import path for callers that have not moved yet.

pub use andromeda_segment::{
    ColdExtentReclaimEvidence, ExtentDescriptor, ExtentFreeRange, ExtentId, ExtentManager,
    ExtentManagerReplayRecord, ExtentState, validate_segment_extent_contiguity,
};
