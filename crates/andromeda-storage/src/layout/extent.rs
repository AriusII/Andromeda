//! Extent allocation layout contracts.
//!
//! Facade re-export. Canonical owner: `crate::extent`. Do not add new
//! definitions here.

pub use crate::{
    ColdExtentReclaimEvidence, ExtentDescriptor, ExtentFreeRange, ExtentId, ExtentManager,
    ExtentManagerReplayRecord, ExtentState, validate_segment_extent_contiguity,
};
