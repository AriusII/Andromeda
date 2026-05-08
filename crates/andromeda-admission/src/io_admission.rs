use andromeda_core::{AndromedaError, PipelineClass, RamSectionRole, ResourceBudget};
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};
use andromeda_storage::{
    CoreIoPlacementDecision, CoreIoPlacementPolicy, CoreIoPlacementRequest, OperationalProfile,
    OperationalProfileMode, StorageWorkloadClass,
};

use crate::{CompletionStatus, InvocationReject};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionIoAdmissionDecision {
    pub trace: DecisionTrace,
    pub profile_mode: OperationalProfileMode,
    pub pipeline_class: PipelineClass,
    pub workload: StorageWorkloadClass,
    pub resource_budget: ResourceBudget,
    pub placement: CoreIoPlacementDecision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionIoAdmissionRequest {
    pub operational_profile: OperationalProfile,
    pub pipeline_class: PipelineClass,
    pub resource_budget: ResourceBudget,
    pub placement_request: CoreIoPlacementRequest,
}

impl ExecutionIoAdmissionRequest {
    pub fn new(
        operational_profile: OperationalProfile,
        pipeline_class: PipelineClass,
        resource_budget: ResourceBudget,
        placement_request: CoreIoPlacementRequest,
    ) -> Self {
        Self {
            operational_profile,
            pipeline_class,
            resource_budget,
            placement_request,
        }
    }

    pub fn validate_admission(
        &self,
        trace_id: TraceId,
    ) -> Result<ExecutionIoAdmissionDecision, InvocationReject> {
        let workload_pipeline = self.placement_request.workload.pipeline_class();
        if self.pipeline_class != workload_pipeline {
            return Err(InvocationReject {
                status: CompletionStatus::ContractRejected,
                reason: format!(
                    "execution IO admission pipeline class {} does not match workload pipeline {}",
                    self.pipeline_class.name(),
                    workload_pipeline.name()
                ),
            });
        }

        if self.pipeline_class.is_critical_path() && self.placement_request.use_gpu {
            return Err(resource_reject(format!(
                "GPU execution is not permitted on critical {} pipeline admission",
                self.pipeline_class.name()
            )));
        }
        if self.pipeline_class.is_critical_path() && self.operational_profile.hardware.gpu.available
        {
            return Err(resource_reject(format!(
                "GPU-permitted operational profiles are not permitted for critical {} pipeline admission",
                self.pipeline_class.name()
            )));
        }

        reject_profile_that_permits_critical_gpu(&self.operational_profile)?;
        self.operational_profile
            .validate()
            .map_err(resource_reject_from_error)?;
        validate_resource_budget(
            self.resource_budget,
            &self.operational_profile,
            self.pipeline_class,
        )?;

        let placement_policy = CoreIoPlacementPolicy::new(
            self.operational_profile.hardware.clone(),
            self.operational_profile.workflow.thresholds,
        );
        let placement = placement_policy
            .plan(self.placement_request)
            .map_err(resource_reject_from_error)?;

        Ok(ExecutionIoAdmissionDecision {
            trace: DecisionTrace {
                trace_id,
                decision: CriticalDecisionKind::ResourceGovernance,
                reason: format!(
                    "execution IO profile {:?}, {} pipeline, resource budget, and storage placement admitted before transaction creation",
                    self.operational_profile.mode,
                    self.pipeline_class.name()
                ),
            },
            profile_mode: self.operational_profile.mode,
            pipeline_class: self.pipeline_class,
            workload: self.placement_request.workload,
            resource_budget: self.resource_budget,
            placement,
        })
    }
}

fn reject_profile_that_permits_critical_gpu(
    profile: &OperationalProfile,
) -> Result<(), InvocationReject> {
    for pipeline in [
        PipelineClass::Commit,
        PipelineClass::WalAppend,
        PipelineClass::Rollback,
        PipelineClass::Recovery,
    ] {
        if profile
            .hardware
            .gpu
            .execution_policy
            .permits_pipeline(pipeline)
        {
            return Err(resource_reject(
                "operational profile must not permit GPU work on commit, WAL, rollback, or recovery critical paths",
            ));
        }
    }

    Ok(())
}

fn validate_resource_budget(
    budget: ResourceBudget,
    profile: &OperationalProfile,
    pipeline: PipelineClass,
) -> Result<(), InvocationReject> {
    if budget.max_memory_bytes == 0 {
        return Err(resource_reject(
            "execution memory budget must not be zero during admission",
        ));
    }
    if budget.max_streams == 0 {
        return Err(resource_reject(
            "execution stream budget must not be zero during admission",
        ));
    }
    if pipeline.is_critical_path() && budget.max_streams != 1 {
        return Err(resource_reject(
            "critical path execution IO admission requires a single bounded stream",
        ));
    }

    let ram = &profile.hardware.ram;
    if ram.total_bytes != 0 {
        let combined = budget
            .max_memory_bytes
            .checked_add(budget.max_temp_bytes)
            .ok_or_else(|| resource_reject("execution resource budget byte totals overflow"))?;
        if combined > ram.total_bytes {
            return Err(resource_reject(
                "execution memory budget plus temp budget exceeds operational profile RAM total",
            ));
        }
    }

    if let Some(max_execution_bytes) = ram.section_budget_bytes(RamSectionRole::Execution)
        && budget.max_memory_bytes > max_execution_bytes
    {
        return Err(resource_reject(
            "execution memory budget exceeds operational profile execution RAM section",
        ));
    }

    if let Some(max_temp_bytes) = ram.section_budget_bytes(RamSectionRole::Temp)
        && budget.max_temp_bytes > max_temp_bytes
    {
        return Err(resource_reject(
            "execution temp budget exceeds operational profile temp RAM section",
        ));
    }

    Ok(())
}

fn resource_reject_from_error(error: AndromedaError) -> InvocationReject {
    resource_reject(error.to_string())
}

fn resource_reject(reason: impl Into<String>) -> InvocationReject {
    InvocationReject {
        status: CompletionStatus::SystemUnavailable,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::{GpuExecutionPolicy, GpuProfile};
    use andromeda_storage::{
        IoLatencyBudget, IoPathBudget, IoPathClass, IoThroughputBudget, PageSize,
        StorageIoBudgetScope,
    };

    fn hot_page_placement_request(
        workload: StorageWorkloadClass,
        use_gpu: bool,
    ) -> CoreIoPlacementRequest {
        CoreIoPlacementRequest::new(
            workload,
            StorageIoBudgetScope::Page(PageSize::KiB16),
            IoPathBudget::new(
                IoPathClass::HotPathNvmeSsd,
                IoLatencyBudget::new(2_000, 2_000, 5_000),
                IoThroughputBudget::new(128 * 1024 * 1024, 128 * 1024 * 1024),
            ),
            use_gpu,
        )
    }

    #[test]
    fn execution_io_admission_accepts_hot_write_foreground_without_gpu() {
        let request = ExecutionIoAdmissionRequest::new(
            OperationalProfile::hot_write(),
            PipelineClass::ForegroundExecution,
            ResourceBudget::new(8 * 1024 * 1024, 1024 * 1024, 2),
            hot_page_placement_request(StorageWorkloadClass::HotAppend, false),
        );

        let decision = request.validate_admission(TraceId::new(20)).unwrap();

        assert_eq!(
            decision.trace.decision,
            CriticalDecisionKind::ResourceGovernance
        );
        assert_eq!(decision.pipeline_class, PipelineClass::ForegroundExecution);
        assert!(!decision.placement.gpu_enabled);
    }

    #[test]
    fn execution_io_admission_rejects_gpu_on_critical_path_before_transaction() {
        let request = ExecutionIoAdmissionRequest::new(
            OperationalProfile::hot_write(),
            PipelineClass::Commit,
            ResourceBudget::new(8 * 1024 * 1024, 1024 * 1024, 2),
            hot_page_placement_request(StorageWorkloadClass::Commit, true),
        );

        let reject = request.validate_admission(TraceId::new(21)).unwrap_err();

        assert_eq!(reject.status, CompletionStatus::SystemUnavailable);
        assert!(reject.reason.contains("GPU"));
        assert!(reject.reason.contains("critical"));
    }

    #[test]
    fn execution_io_admission_rejects_profiles_that_permit_gpu_on_critical_paths() {
        let request = ExecutionIoAdmissionRequest::new(
            OperationalProfile::analytics_off_critical_path(),
            PipelineClass::Commit,
            ResourceBudget::new(8 * 1024 * 1024, 1024 * 1024, 1),
            hot_page_placement_request(StorageWorkloadClass::Commit, false),
        );

        let reject = request.validate_admission(TraceId::new(22)).unwrap_err();

        assert_eq!(reject.status, CompletionStatus::SystemUnavailable);
        assert!(reject.reason.contains("GPU-permitted operational profiles"));
        assert!(reject.reason.contains("critical"));
    }

    #[test]
    fn execution_io_admission_rejects_non_analytics_gpu_profile() {
        let mut profile = OperationalProfile::hot_write();
        profile.hardware.gpu = GpuProfile {
            available: true,
            execution_policy: GpuExecutionPolicy::OffCriticalPathOnly,
        };

        let request = ExecutionIoAdmissionRequest::new(
            profile,
            PipelineClass::ForegroundExecution,
            ResourceBudget::new(8 * 1024 * 1024, 1024 * 1024, 2),
            hot_page_placement_request(StorageWorkloadClass::HotAppend, false),
        );

        let reject = request.validate_admission(TraceId::new(25)).unwrap_err();

        assert_eq!(reject.status, CompletionStatus::SystemUnavailable);
        assert!(reject.reason.contains("non-analytics operational profiles"));
    }

    #[test]
    fn execution_io_admission_rejects_pipeline_workload_mismatch() {
        let request = ExecutionIoAdmissionRequest::new(
            OperationalProfile::hot_write(),
            PipelineClass::Recovery,
            ResourceBudget::new(8 * 1024 * 1024, 1024 * 1024, 2),
            hot_page_placement_request(StorageWorkloadClass::HotAppend, false),
        );

        let reject = request.validate_admission(TraceId::new(23)).unwrap_err();

        assert_eq!(reject.status, CompletionStatus::ContractRejected);
        assert!(reject.reason.contains("pipeline class"));
    }

    #[test]
    fn execution_io_admission_rejects_resource_budget_over_profile_ram() {
        let request = ExecutionIoAdmissionRequest::new(
            OperationalProfile::hot_write(),
            PipelineClass::ForegroundExecution,
            ResourceBudget::new(600 * 1024 * 1024, 1024 * 1024, 2),
            hot_page_placement_request(StorageWorkloadClass::HotAppend, false),
        );

        let reject = request.validate_admission(TraceId::new(24)).unwrap_err();

        assert_eq!(reject.status, CompletionStatus::SystemUnavailable);
        assert!(reject.reason.contains("execution memory budget"));
    }
}
