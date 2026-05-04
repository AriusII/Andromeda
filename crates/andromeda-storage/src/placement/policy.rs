use andromeda_core::{AndromedaResult, HardwareProfile, PipelineClass};

use crate::{HotColdIoThresholds, IoPathBudget, IoPathClass};

use super::{
    resource_error, storage_error, PlacementDecision, StorageIoBudgetScope, StorageTier,
    StorageWorkloadClass,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreIoPlacementRequest {
    pub workload: StorageWorkloadClass,
    pub io_budget_scope: StorageIoBudgetScope,
    pub path_budget: IoPathBudget,
    pub use_gpu: bool,
}

impl CoreIoPlacementRequest {
    pub const fn new(
        workload: StorageWorkloadClass,
        io_budget_scope: StorageIoBudgetScope,
        path_budget: IoPathBudget,
        use_gpu: bool,
    ) -> Self {
        Self {
            workload,
            io_budget_scope,
            path_budget,
            use_gpu,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreIoPlacementDecision {
    pub workload: StorageWorkloadClass,
    pub placement: PlacementDecision,
    pub pipeline_class: PipelineClass,
    pub io_use_class: crate::IoUseClass,
    pub path_budget: IoPathBudget,
    pub gpu_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreIoPlacementPolicy {
    pub hardware: HardwareProfile,
    pub thresholds: HotColdIoThresholds,
}

pub type StoragePlacementPolicy = CoreIoPlacementPolicy;

impl CoreIoPlacementPolicy {
    pub const fn new(hardware: HardwareProfile, thresholds: HotColdIoThresholds) -> Self {
        Self {
            hardware,
            thresholds,
        }
    }

    pub const fn conservative() -> Self {
        Self {
            hardware: HardwareProfile::conservative(),
            thresholds: HotColdIoThresholds::conservative(),
        }
    }

    pub fn plan(
        &self,
        request: CoreIoPlacementRequest,
    ) -> AndromedaResult<CoreIoPlacementDecision> {
        self.hardware.validate_ram_budgets()?;

        let pipeline_class = request.workload.pipeline_class();
        if request.use_gpu {
            self.hardware.validate_gpu_pipeline(pipeline_class)?;
        }

        let placement = request.workload.placement_decision()?;
        placement.validate()?;

        validate_core_placement_invariants(request.workload, &placement)?;
        validate_ram_budget_for_workload(
            &self.hardware,
            request.workload,
            request.io_budget_scope,
        )?;

        let expected_path = io_path_for_tier(placement.target_tier);
        if request.path_budget.path_class != expected_path {
            return Err(storage_error(
                "IO budget path class must match the selected storage placement tier",
            ));
        }

        let io_use_class = request.workload.io_use_class();
        request
            .io_budget_scope
            .validate(io_use_class, request.path_budget, self.thresholds)?;

        Ok(CoreIoPlacementDecision {
            workload: request.workload,
            placement,
            pipeline_class,
            io_use_class,
            path_budget: request.path_budget,
            gpu_enabled: request.use_gpu,
        })
    }
}

impl Default for CoreIoPlacementPolicy {
    fn default() -> Self {
        Self::conservative()
    }
}

fn validate_core_placement_invariants(
    workload: StorageWorkloadClass,
    placement: &PlacementDecision,
) -> AndromedaResult<()> {
    match workload {
        StorageWorkloadClass::Commit
        | StorageWorkloadClass::WalAppend
        | StorageWorkloadClass::Rollback
        | StorageWorkloadClass::Recovery
        | StorageWorkloadClass::HotAppend => {
            if placement.target_tier != StorageTier::HotStore {
                return Err(storage_error(
                    "commit-critical and hot append workloads must target HotStore",
                ));
            }
        }
        StorageWorkloadClass::RamWorkingSet => {
            if placement.target_tier != StorageTier::Ram {
                return Err(storage_error("RAM working set must target RAM placement"));
            }
        }
        StorageWorkloadClass::ColdRead | StorageWorkloadClass::ColdPublication => {
            if placement.target_tier != StorageTier::ColdStore {
                return Err(storage_error(
                    "cold reads and publication must target ColdStore",
                ));
            }
            if placement.mutation_allowed {
                return Err(storage_error(
                    "cold reads and publication must not allow mutation",
                ));
            }
        }
    }
    Ok(())
}

fn validate_ram_budget_for_workload(
    hardware: &HardwareProfile,
    workload: StorageWorkloadClass,
    io_budget_scope: StorageIoBudgetScope,
) -> AndromedaResult<()> {
    if let Some(role) = workload.ram_section_role() {
        let bytes = io_budget_scope.logical_bytes();
        if hardware.ram.total_bytes != 0 && bytes > hardware.ram.total_bytes {
            return Err(resource_error(
                "storage workload logical bytes exceed declared RAM total",
            ));
        }
        if let Some(max_bytes) = hardware.ram.section_budget_bytes(role) {
            if bytes > max_bytes {
                return Err(resource_error(
                    "storage workload logical bytes exceed RAM section budget",
                ));
            }
        }
    }

    Ok(())
}

const fn io_path_for_tier(tier: StorageTier) -> IoPathClass {
    match tier {
        StorageTier::Ram => IoPathClass::CpuRam,
        StorageTier::HotStore => IoPathClass::HotPathNvmeSsd,
        StorageTier::ColdStore => IoPathClass::ColdPathHdd,
    }
}
