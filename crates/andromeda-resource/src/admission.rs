use std::{error::Error, fmt};

use andromeda_hardware::{HardwareProfile, PipelineClass, RamSectionRole};

use crate::{ResourceBudget, ResourceLimitField};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionResourceAdmissionRequest {
    pub pipeline_class: PipelineClass,
    pub budget: ResourceBudget,
    pub gpu_requested: bool,
}

impl ExecutionResourceAdmissionRequest {
    pub const fn new(
        pipeline_class: PipelineClass,
        budget: ResourceBudget,
        gpu_requested: bool,
    ) -> Self {
        Self {
            pipeline_class,
            budget,
            gpu_requested,
        }
    }

    pub fn validate_no_critical_gpu(
        &self,
        hardware: &HardwareProfile,
    ) -> Result<(), ResourceAdmissionRejection> {
        if self.pipeline_class.is_critical_path() && self.gpu_requested {
            return Err(ResourceAdmissionRejection::CriticalPathGpuRequested {
                pipeline: self.pipeline_class,
            });
        }

        if self.pipeline_class.is_critical_path() && hardware.gpu.available {
            return Err(
                ResourceAdmissionRejection::CriticalPathGpuProfileAvailable {
                    pipeline: self.pipeline_class,
                },
            );
        }

        for pipeline in CRITICAL_GPU_PIPELINES {
            if hardware.gpu.execution_policy.permits_pipeline(pipeline) {
                return Err(ResourceAdmissionRejection::CriticalPathGpuProfilePermitted);
            }
        }

        Ok(())
    }

    pub fn admit(
        &self,
        hardware: &HardwareProfile,
    ) -> Result<ExecutionResourceAdmissionDecision, ResourceAdmissionRejection> {
        self.validate_no_critical_gpu(hardware)?;
        let total_budget_bytes = validate_budget(self.budget, &hardware.ram, self.pipeline_class)?;

        Ok(ExecutionResourceAdmissionDecision {
            pipeline_class: self.pipeline_class,
            budget: self.budget,
            evidence: ResourceAdmissionEvidence {
                memory_bytes: self.budget.max_memory_bytes.bytes(),
                temp_bytes: self.budget.max_temp_bytes.bytes(),
                total_budget_bytes,
                streams: self.budget.max_streams.streams(),
                ram_total_bytes: hardware.ram.total_bytes,
                execution_ram_section_bytes: hardware
                    .ram
                    .section_budget_bytes(RamSectionRole::Execution),
                temp_ram_section_bytes: hardware.ram.section_budget_bytes(RamSectionRole::Temp),
                gpu_requested: self.gpu_requested,
            },
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionResourceAdmissionDecision {
    pub pipeline_class: PipelineClass,
    pub budget: ResourceBudget,
    pub evidence: ResourceAdmissionEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceAdmissionEvidence {
    pub memory_bytes: u64,
    pub temp_bytes: u64,
    pub total_budget_bytes: u64,
    pub streams: u32,
    pub ram_total_bytes: u64,
    pub execution_ram_section_bytes: Option<u64>,
    pub temp_ram_section_bytes: Option<u64>,
    pub gpu_requested: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceAdmissionRejection {
    CriticalPathGpuRequested {
        pipeline: PipelineClass,
    },
    CriticalPathGpuProfileAvailable {
        pipeline: PipelineClass,
    },
    CriticalPathGpuProfilePermitted,
    ZeroBudget {
        field: ResourceLimitField,
    },
    CriticalPathStreamCount {
        streams: u32,
    },
    ByteBudgetOverflow,
    BudgetExceedsRamTotal {
        budget: u64,
        limit: u64,
    },
    BudgetExceedsRamSection {
        field: ResourceLimitField,
        section: RamSectionRole,
        budget: u64,
        limit: u64,
    },
}

impl fmt::Display for ResourceAdmissionRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CriticalPathGpuRequested { pipeline } => write!(
                f,
                "GPU execution is not permitted on critical {} pipeline admission",
                pipeline.name()
            ),
            Self::CriticalPathGpuProfileAvailable { pipeline } => write!(
                f,
                "GPU-permitted operational profiles are not permitted for critical {} pipeline admission",
                pipeline.name()
            ),
            Self::CriticalPathGpuProfilePermitted => f.write_str(
                "operational profile must not permit GPU work on commit, WAL, rollback, recovery, MVCC visibility, catalog publication, or security critical paths",
            ),
            Self::ZeroBudget { field } => match field {
                ResourceLimitField::MemoryBytes => {
                    f.write_str("execution memory budget must not be zero during admission")
                }
                ResourceLimitField::TempBytes => {
                    f.write_str("execution temp budget must not be zero during admission")
                }
                ResourceLimitField::StreamCount => {
                    f.write_str("execution stream budget must not be zero during admission")
                }
            },
            Self::CriticalPathStreamCount { .. } => {
                f.write_str("critical path execution IO admission requires a single bounded stream")
            }
            Self::ByteBudgetOverflow => {
                f.write_str("execution resource budget byte totals overflow")
            }
            Self::BudgetExceedsRamTotal { .. } => f.write_str(
                "execution memory budget plus temp budget exceeds operational profile RAM total",
            ),
            Self::BudgetExceedsRamSection { field, .. } => match field {
                ResourceLimitField::MemoryBytes => f.write_str(
                    "execution memory budget exceeds operational profile execution RAM section",
                ),
                ResourceLimitField::TempBytes => f.write_str(
                    "execution temp budget exceeds operational profile temp RAM section",
                ),
                ResourceLimitField::StreamCount => {
                    f.write_str("execution stream budget exceeds operational profile stream limit")
                }
            },
        }
    }
}

impl Error for ResourceAdmissionRejection {}

fn validate_budget(
    budget: ResourceBudget,
    ram: &andromeda_hardware::RamProfile,
    pipeline: PipelineClass,
) -> Result<u64, ResourceAdmissionRejection> {
    if budget.max_memory_bytes.bytes() == 0 {
        return Err(ResourceAdmissionRejection::ZeroBudget {
            field: ResourceLimitField::MemoryBytes,
        });
    }
    if budget.max_streams.streams() == 0 {
        return Err(ResourceAdmissionRejection::ZeroBudget {
            field: ResourceLimitField::StreamCount,
        });
    }
    if pipeline.is_critical_path() && budget.max_streams.streams() != 1 {
        return Err(ResourceAdmissionRejection::CriticalPathStreamCount {
            streams: budget.max_streams.streams(),
        });
    }

    let total_budget_bytes = budget
        .total_bytes()
        .map_err(|_| ResourceAdmissionRejection::ByteBudgetOverflow)?;

    if ram.total_bytes != 0 && total_budget_bytes > ram.total_bytes {
        return Err(ResourceAdmissionRejection::BudgetExceedsRamTotal {
            budget: total_budget_bytes,
            limit: ram.total_bytes,
        });
    }

    if let Some(max_execution_bytes) = ram.section_budget_bytes(RamSectionRole::Execution)
        && budget.max_memory_bytes.bytes() > max_execution_bytes
    {
        return Err(ResourceAdmissionRejection::BudgetExceedsRamSection {
            field: ResourceLimitField::MemoryBytes,
            section: RamSectionRole::Execution,
            budget: budget.max_memory_bytes.bytes(),
            limit: max_execution_bytes,
        });
    }

    if let Some(max_temp_bytes) = ram.section_budget_bytes(RamSectionRole::Temp)
        && budget.max_temp_bytes.bytes() > max_temp_bytes
    {
        return Err(ResourceAdmissionRejection::BudgetExceedsRamSection {
            field: ResourceLimitField::TempBytes,
            section: RamSectionRole::Temp,
            budget: budget.max_temp_bytes.bytes(),
            limit: max_temp_bytes,
        });
    }

    Ok(total_budget_bytes)
}

const CRITICAL_GPU_PIPELINES: [PipelineClass; 7] = [
    PipelineClass::Commit,
    PipelineClass::WalAppend,
    PipelineClass::Rollback,
    PipelineClass::Recovery,
    PipelineClass::MvccVisibility,
    PipelineClass::CatalogPublication,
    PipelineClass::SecurityCriticalPath,
];

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_hardware::{
        CpuProfile, GpuProfile, HardwareArchitecture, RamProfile, RamSectionBudget,
    };

    fn hardware(total_bytes: u64) -> HardwareProfile {
        let cpu = CpuProfile::conservative();
        HardwareProfile {
            architecture: HardwareArchitecture::Unknown,
            has_simd: false,
            has_direct_io: false,
            cpu,
            ram: RamProfile::new(
                total_bytes,
                vec![
                    RamSectionBudget::new(RamSectionRole::Execution, total_bytes / 4),
                    RamSectionBudget::new(RamSectionRole::Temp, total_bytes / 8),
                ],
            ),
            gpu: GpuProfile::disabled(),
        }
    }

    #[test]
    fn resource_admission_returns_typed_evidence() {
        let request = ExecutionResourceAdmissionRequest::new(
            PipelineClass::ForegroundExecution,
            ResourceBudget::new(16, 8, 2),
            false,
        );

        let decision = request.admit(&hardware(128)).unwrap();

        assert_eq!(decision.pipeline_class, PipelineClass::ForegroundExecution);
        assert_eq!(decision.evidence.memory_bytes, 16);
        assert_eq!(decision.evidence.temp_bytes, 8);
        assert_eq!(decision.evidence.total_budget_bytes, 24);
        assert_eq!(decision.evidence.streams, 2);
        assert!(!decision.evidence.gpu_requested);
    }

    #[test]
    fn resource_admission_rejects_zero_memory_or_streams() {
        let hardware = hardware(128);

        assert_eq!(
            ExecutionResourceAdmissionRequest::new(
                PipelineClass::ForegroundExecution,
                ResourceBudget::new(0, 8, 2),
                false,
            )
            .admit(&hardware)
            .unwrap_err(),
            ResourceAdmissionRejection::ZeroBudget {
                field: ResourceLimitField::MemoryBytes
            }
        );
        assert_eq!(
            ExecutionResourceAdmissionRequest::new(
                PipelineClass::ForegroundExecution,
                ResourceBudget::new(16, 8, 0),
                false,
            )
            .admit(&hardware)
            .unwrap_err(),
            ResourceAdmissionRejection::ZeroBudget {
                field: ResourceLimitField::StreamCount
            }
        );
    }

    #[test]
    fn critical_resource_admission_requires_single_stream() {
        let reject = ExecutionResourceAdmissionRequest::new(
            PipelineClass::Commit,
            ResourceBudget::new(16, 8, 2),
            false,
        )
        .admit(&hardware(128))
        .unwrap_err();

        assert_eq!(
            reject,
            ResourceAdmissionRejection::CriticalPathStreamCount { streams: 2 }
        );
    }

    #[test]
    fn resource_admission_rejects_ram_overages() {
        let hardware = hardware(128);

        assert_eq!(
            ExecutionResourceAdmissionRequest::new(
                PipelineClass::ForegroundExecution,
                ResourceBudget::new(120, 16, 2),
                false,
            )
            .admit(&hardware)
            .unwrap_err(),
            ResourceAdmissionRejection::BudgetExceedsRamTotal {
                budget: 136,
                limit: 128
            }
        );
        assert_eq!(
            ExecutionResourceAdmissionRequest::new(
                PipelineClass::ForegroundExecution,
                ResourceBudget::new(33, 8, 2),
                false,
            )
            .admit(&hardware)
            .unwrap_err(),
            ResourceAdmissionRejection::BudgetExceedsRamSection {
                field: ResourceLimitField::MemoryBytes,
                section: RamSectionRole::Execution,
                budget: 33,
                limit: 32
            }
        );
    }

    #[test]
    fn critical_resource_admission_rejects_gpu_presence() {
        let mut hardware = hardware(128);
        hardware.gpu = GpuProfile::batch_analytics_only();

        assert_eq!(
            ExecutionResourceAdmissionRequest::new(
                PipelineClass::Commit,
                ResourceBudget::new(16, 8, 1),
                false,
            )
            .validate_no_critical_gpu(&hardware)
            .unwrap_err(),
            ResourceAdmissionRejection::CriticalPathGpuProfileAvailable {
                pipeline: PipelineClass::Commit
            }
        );

        hardware.gpu = GpuProfile::disabled();
        assert_eq!(
            ExecutionResourceAdmissionRequest::new(
                PipelineClass::Commit,
                ResourceBudget::new(16, 8, 1),
                true,
            )
            .validate_no_critical_gpu(&hardware)
            .unwrap_err(),
            ResourceAdmissionRejection::CriticalPathGpuRequested {
                pipeline: PipelineClass::Commit
            }
        );
    }
}
