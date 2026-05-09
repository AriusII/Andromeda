mod descriptor;
mod error;
mod free_range;
mod manager;
mod page_range;
mod reclaim;
mod replay;
mod segment_contiguity;
#[cfg(test)]
mod tests;

pub use andromeda_storage_page::ExtentId;
pub use descriptor::{ExtentDescriptor, ExtentState};
pub use free_range::ExtentFreeRange;
pub use manager::ExtentManager;
pub use reclaim::ColdExtentReclaimEvidence;
pub use replay::ExtentManagerReplayRecord;
pub use segment_contiguity::validate_segment_extent_contiguity;
