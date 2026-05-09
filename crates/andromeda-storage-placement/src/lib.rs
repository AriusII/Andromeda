#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Storage Placement

Storage placement, IO budget, workload, and operational profile admission
contracts. These policies are runtime-free guardrails and do not own durable
storage state.
"#]

mod operational_profile;
mod placement;
mod publication;

pub use operational_profile::{IoWorkflowProfile, OperationalProfile, OperationalProfileMode};
pub use placement::{
    CoreIoPlacementDecision, CoreIoPlacementPolicy, CoreIoPlacementRequest, DataTemperature,
    HotColdIoThresholds, IoLatencyBudget, IoPathBudget, IoPathClass, IoThroughputBudget,
    IoUseClass, PageIoBudget, PipelineStage, PlacementDecision, ReadFallbackPolicy,
    SegmentIoBudget, StorageIoBudgetScope, StoragePlacementPolicy, StorageTier,
    StorageWorkloadClass,
};
pub use publication::{ColdSegmentPublicationPlan, validate_cold_segment_publication_boundary};
