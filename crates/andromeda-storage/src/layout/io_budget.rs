//! IO lane budget contracts for page and segment placement.
//!
//! Facade re-export. Canonical owner: `crate::placement`. Do not add new
//! definitions here.

pub use crate::{
    HotColdIoThresholds, IoLatencyBudget, IoPathBudget, IoPathClass, IoThroughputBudget,
    IoUseClass, PageIoBudget, SegmentIoBudget,
};
