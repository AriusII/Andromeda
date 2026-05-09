//! Storage-coupled cold segment publication planning.
//!
//! These contracts bridge storage placement policy with manifest publication
//! boundaries. They validate that a sealed segment is eligible for immutable
//! cold publication, but the plan is advisory admission evidence and does not
//! publish, persist, or mutate segment or manifest truth by itself.

use andromeda_error::AndromedaResult;
use andromeda_hardware::PipelineClass;

use crate::{
    CoreIoPlacementDecision, CoreIoPlacementPolicy, CoreIoPlacementRequest, IoPathBudget,
    IoPathClass, IoUseClass, PipelineStage, PlacementDecision, SegmentDescriptor,
    StorageIoBudgetScope, StorageTier, StorageWorkloadClass,
};

use super::storage_error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColdSegmentPublicationPlan {
    pub sealed_segment: SegmentDescriptor,
    pub decision: CoreIoPlacementDecision,
}

impl ColdSegmentPublicationPlan {
    pub fn new(
        sealed_segment: SegmentDescriptor,
        policy: &CoreIoPlacementPolicy,
        path_budget: IoPathBudget,
    ) -> AndromedaResult<Self> {
        let segment_bytes = cold_publication_segment_bytes(&sealed_segment)?;
        let decision = policy.plan(CoreIoPlacementRequest::new(
            StorageWorkloadClass::ColdPublication,
            StorageIoBudgetScope::Segment {
                bytes: segment_bytes,
            },
            path_budget,
            false,
        ))?;

        validate_cold_segment_publication_boundary(&sealed_segment, &decision)?;

        Ok(Self {
            sealed_segment,
            decision,
        })
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        validate_cold_segment_publication_boundary(&self.sealed_segment, &self.decision)
    }
}

pub fn validate_cold_segment_publication_boundary(
    sealed_segment: &SegmentDescriptor,
    decision: &CoreIoPlacementDecision,
) -> AndromedaResult<()> {
    let expected_placement = PlacementDecision::publish_cold_segment(sealed_segment)?;

    if decision.workload != StorageWorkloadClass::ColdPublication {
        return Err(storage_error(
            "ColdStore segment publication requires the cold publication workload class",
        ));
    }
    if decision.pipeline_class != PipelineClass::BackgroundMaintenance {
        return Err(storage_error(
            "ColdStore segment publication must run on the background publication pipeline",
        ));
    }
    if decision.io_use_class != IoUseClass::ColdSegmentPath {
        return Err(storage_error(
            "ColdStore segment publication requires the cold segment IO budget class",
        ));
    }
    if decision.path_budget.path_class != IoPathClass::ColdPathHdd {
        return Err(storage_error(
            "ColdStore segment publication requires a cold-path IO budget",
        ));
    }
    if decision.gpu_enabled {
        return Err(storage_error(
            "ColdStore segment publication plan must not enable GPU execution",
        ));
    }
    if decision.placement.temperature != expected_placement.temperature
        || decision.placement.target_tier != expected_placement.target_tier
        || decision.placement.pipeline_stage != expected_placement.pipeline_stage
        || decision.placement.read_fallback != expected_placement.read_fallback
        || decision.placement.mutation_allowed != expected_placement.mutation_allowed
    {
        return Err(storage_error(
            "ColdStore segment publication must use the sealed hot segment publication placement",
        ));
    }
    if decision.placement.pipeline_stage != PipelineStage::PublishColdStore
        || decision.placement.target_tier != StorageTier::ColdStore
        || decision.placement.mutation_allowed
    {
        return Err(storage_error(
            "ColdStore segment publication must be immutable and target the cold publication stage",
        ));
    }

    Ok(())
}

fn cold_publication_segment_bytes(sealed_segment: &SegmentDescriptor) -> AndromedaResult<u64> {
    sealed_segment.validate()?;
    u64::from(sealed_segment.page_count)
        .checked_mul(u64::from(sealed_segment.page_size.bytes()))
        .ok_or_else(|| storage_error("cold publication segment byte size overflows u64"))
}
