//! Hot/cold placement and RAM -> HotStore -> ColdStore pipeline contracts.

pub use crate::{
    CoreIoPlacementDecision, CoreIoPlacementPolicy, CoreIoPlacementRequest, DataTemperature,
    PipelineStage, PlacementDecision, ReadFallbackPolicy, StorageIoBudgetScope,
    StoragePlacementPolicy, StorageTier, StorageWorkloadClass,
};
