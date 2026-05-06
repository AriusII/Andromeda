//! Hot/cold placement and RAM -> HotStore -> ColdStore pipeline contracts.
//!
//! Facade re-export. Canonical owner: `crate::placement`. Do not add new
//! definitions here.

pub use crate::{
    CoreIoPlacementDecision, CoreIoPlacementPolicy, CoreIoPlacementRequest, DataTemperature,
    PipelineStage, PlacementDecision, ReadFallbackPolicy, StorageIoBudgetScope,
    StoragePlacementPolicy, StorageTier, StorageWorkloadClass,
};
