//! IO lane budget contracts for page and segment placement.
//!
//! Facade re-export. Canonical owner: [`crate::io_budget`]. Do not add new
//! definitions here.

pub use crate::{
    HotColdIoThresholds, IoLatencyBudget, IoPathBudget, IoPathClass, IoThroughputBudget,
    IoUseClass, PageIoBudget, SegmentIoBudget,
};
