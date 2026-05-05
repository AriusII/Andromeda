mod budget;
mod decision;
mod io_budget;
mod policy;
mod types;
mod workload;

pub use self::budget::StorageIoBudgetScope;
pub use self::decision::PlacementDecision;
pub use self::io_budget::{
    HotColdIoThresholds, IoLatencyBudget, IoPathBudget, IoPathClass, IoThroughputBudget,
    IoUseClass, PageIoBudget, SegmentIoBudget,
};
pub use self::policy::{
    CoreIoPlacementDecision, CoreIoPlacementPolicy, CoreIoPlacementRequest, StoragePlacementPolicy,
};
pub use self::types::{DataTemperature, PipelineStage, ReadFallbackPolicy, StorageTier};
pub use self::workload::StorageWorkloadClass;

use andromeda_core::{AndromedaError, AndromedaErrorKind};

fn resource_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Resource, message)
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
