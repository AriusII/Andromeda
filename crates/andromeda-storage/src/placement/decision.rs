use andromeda_core::AndromedaResult;

use crate::{SegmentDescriptor, SegmentMutation, SegmentState};

use super::{DataTemperature, PipelineStage, ReadFallbackPolicy, StorageTier, storage_error};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacementDecision {
    pub temperature: DataTemperature,
    pub target_tier: StorageTier,
    pub pipeline_stage: PipelineStage,
    pub read_fallback: ReadFallbackPolicy,
    pub mutation_allowed: bool,
    pub reason: &'static str,
}

impl PlacementDecision {
    pub const fn read(temperature: DataTemperature) -> Self {
        Self {
            temperature,
            target_tier: temperature.preferred_tier(),
            pipeline_stage: PipelineStage::ServeRead,
            read_fallback: ReadFallbackPolicy::for_temperature(temperature),
            mutation_allowed: false,
            reason: "read placement follows data temperature and explicit fallback policy",
        }
    }

    pub fn append(temperature: DataTemperature) -> AndromedaResult<Self> {
        if temperature.is_cold() {
            return Err(storage_error(
                "append placement cannot target ColdStore; cold data must remain immutable",
            ));
        }

        Ok(Self {
            temperature,
            target_tier: StorageTier::HotStore,
            pipeline_stage: PipelineStage::AppendHotStore,
            read_fallback: ReadFallbackPolicy::for_temperature(temperature),
            mutation_allowed: true,
            reason: "append enters the RAM to HotStore pipeline and never appends to ColdStore",
        })
    }

    pub fn mutation(
        descriptor: &SegmentDescriptor,
        mutation: SegmentMutation,
        temperature: DataTemperature,
    ) -> AndromedaResult<Self> {
        descriptor.validate_mutation(mutation)?;
        match mutation {
            SegmentMutation::AppendExtent => Self::append(temperature),
            SegmentMutation::UpdatePageInPlace | SegmentMutation::SplitSegment => {
                if descriptor.state == SegmentState::Sealed {
                    return Err(storage_error(
                        "sealed hot segment rejects update-in-place and split mutations",
                    ));
                }
                Ok(Self {
                    temperature,
                    target_tier: StorageTier::Ram,
                    pipeline_stage: PipelineStage::BufferInRam,
                    read_fallback: ReadFallbackPolicy::RamThenHotThenCold,
                    mutation_allowed: true,
                    reason: "mutable page work is staged in RAM before HotStore placement",
                })
            }
        }
    }

    pub fn seal_hot_segment(descriptor: &SegmentDescriptor) -> AndromedaResult<Self> {
        descriptor.validate()?;
        if descriptor.state != SegmentState::BuildingHotSnapshot {
            return Err(storage_error(
                "only a building hot snapshot segment can enter the HotStore seal stage",
            ));
        }

        Ok(Self {
            temperature: DataTemperature::Cooling,
            target_tier: StorageTier::HotStore,
            pipeline_stage: PipelineStage::SealHotStoreSegment,
            read_fallback: ReadFallbackPolicy::HotThenCold,
            mutation_allowed: false,
            reason: "sealing closes HotStore mutation before possible ColdStore publication",
        })
    }

    pub fn publish_cold_segment(descriptor: &SegmentDescriptor) -> AndromedaResult<Self> {
        descriptor.validate()?;
        if descriptor.state != SegmentState::Sealed {
            return Err(storage_error(
                "ColdStore publication requires a sealed HotStore segment descriptor",
            ));
        }
        if descriptor.snapshot_id.is_none() {
            return Err(storage_error(
                "ColdStore publication requires a nonzero snapshot reference",
            ));
        }

        Ok(Self {
            temperature: DataTemperature::Cold,
            target_tier: StorageTier::ColdStore,
            pipeline_stage: PipelineStage::PublishColdStore,
            read_fallback: ReadFallbackPolicy::ColdOnly,
            mutation_allowed: false,
            reason: "ColdStore publication is immutable and only follows HotStore sealing",
        })
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.target_tier == StorageTier::ColdStore && self.mutation_allowed {
            return Err(storage_error("ColdStore placement must not allow mutation"));
        }
        if self.pipeline_stage == PipelineStage::PublishColdStore
            && self.target_tier != StorageTier::ColdStore
        {
            return Err(storage_error(
                "ColdStore publication stage must target the ColdStore tier",
            ));
        }
        if self.pipeline_stage == PipelineStage::AppendHotStore
            && self.target_tier == StorageTier::ColdStore
        {
            return Err(storage_error("append stage must not target ColdStore"));
        }
        if !self.read_fallback.allows_tier(self.target_tier) {
            return Err(storage_error(
                "read fallback policy must include the placement target tier",
            ));
        }
        Ok(())
    }
}
