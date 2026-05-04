use andromeda_core::{AndromedaResult, PipelineClass, RamSectionRole};

use crate::IoUseClass;

use super::{DataTemperature, PipelineStage, PlacementDecision, ReadFallbackPolicy, StorageTier};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageWorkloadClass {
    Commit,
    WalAppend,
    Rollback,
    Recovery,
    RamWorkingSet,
    HotAppend,
    ColdRead,
    ColdPublication,
}

impl StorageWorkloadClass {
    pub const fn pipeline_class(self) -> PipelineClass {
        match self {
            Self::Commit => PipelineClass::Commit,
            Self::WalAppend => PipelineClass::WalAppend,
            Self::Rollback => PipelineClass::Rollback,
            Self::Recovery => PipelineClass::Recovery,
            Self::RamWorkingSet | Self::HotAppend | Self::ColdRead => {
                PipelineClass::ForegroundExecution
            }
            Self::ColdPublication => PipelineClass::BackgroundMaintenance,
        }
    }

    pub const fn io_use_class(self) -> IoUseClass {
        match self {
            Self::Commit | Self::WalAppend | Self::Rollback | Self::Recovery => {
                IoUseClass::CommitCriticalHotPath
            }
            Self::RamWorkingSet | Self::HotAppend => IoUseClass::OnlineHotPath,
            Self::ColdRead | Self::ColdPublication => IoUseClass::ColdSegmentPath,
        }
    }

    pub const fn ram_section_role(self) -> Option<RamSectionRole> {
        match self {
            Self::RamWorkingSet => Some(RamSectionRole::Cache),
            Self::Commit | Self::WalAppend | Self::HotAppend => Some(RamSectionRole::Io),
            Self::Rollback | Self::Recovery => Some(RamSectionRole::Execution),
            Self::ColdRead | Self::ColdPublication => None,
        }
    }

    pub(super) fn placement_decision(self) -> AndromedaResult<PlacementDecision> {
        match self {
            Self::Commit | Self::WalAppend | Self::Rollback | Self::Recovery | Self::HotAppend => {
                PlacementDecision::append(DataTemperature::Hot)
            }
            Self::RamWorkingSet => Ok(PlacementDecision::read(DataTemperature::RamWorkingSet)),
            Self::ColdRead => Ok(PlacementDecision::read(DataTemperature::Cold)),
            Self::ColdPublication => Ok(PlacementDecision {
                temperature: DataTemperature::Cold,
                target_tier: StorageTier::ColdStore,
                pipeline_stage: PipelineStage::PublishColdStore,
                read_fallback: ReadFallbackPolicy::ColdOnly,
                mutation_allowed: false,
                reason: "policy maps cold publication to immutable ColdStore placement",
            }),
        }
    }
}
