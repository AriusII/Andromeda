use andromeda_error::AndromedaError;
use andromeda_hardware::PipelineClass;
use andromeda_observability::{CriticalDecisionKind, DecisionTrace, TraceId};
use andromeda_resource::{
    ExecutionResourceAdmissionDecision, ExecutionResourceAdmissionRequest,
    ResourceAdmissionRejection, ResourceBudget,
};
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
    pub resource_decision: ExecutionResourceAdmissionDecision,
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
        resource_budget: impl Into<ResourceBudget>,
        placement_request: CoreIoPlacementRequest,
    ) -> Self {
        Self {
            operational_profile,
            pipeline_class,
            resource_budget: resource_budget.into(),
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

        let resource_admission = ExecutionResourceAdmissionRequest::new(
            self.pipeline_class,
            self.resource_budget,
            self.placement_request.use_gpu,
        );

        resource_admission
            .validate_no_critical_gpu(&self.operational_profile.hardware)
            .map_err(resource_reject_from_admission)?;
        self.operational_profile
            .validate()
            .map_err(resource_reject_from_error)?;
        let resource_decision = resource_admission
            .admit(&self.operational_profile.hardware)
            .map_err(resource_reject_from_admission)?;

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
            resource_decision,
            placement,
        })
    }
}

fn resource_reject_from_error(error: AndromedaError) -> InvocationReject {
    resource_reject(error.to_string())
}

fn resource_reject_from_admission(error: ResourceAdmissionRejection) -> InvocationReject {
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
    use andromeda_hardware::{GpuExecutionPolicy, GpuProfile};
    use andromeda_storage::{
        IoLatencyBudget, IoPathBudget, IoPathClass, IoThroughputBudget, StorageIoBudgetScope,
    };
    use andromeda_storage_page::PageSize;

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
        assert_eq!(decision.resource_decision.budget, decision.resource_budget);
        assert_eq!(decision.resource_decision.evidence.streams, 2);
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
