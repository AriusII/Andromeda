use std::{error::Error, fmt};

use andromeda_hardware::{HardwareProfile, PipelineClass, RamSectionRole};
use andromeda_types::ProcedureId;

use crate::{ResourceBudget, ResourceBudgetScope, ResourceLimitField, ResourceScopeResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResourceAdmissionRequest {
    pub scope: ResourceBudgetScope,
    pub pipeline_class: PipelineClass,
    pub budget: ResourceBudget,
    pub gpu_requested: bool,
}

impl ExecutionResourceAdmissionRequest {
    pub fn new(
        scope: ResourceBudgetScope,
        pipeline_class: PipelineClass,
        budget: ResourceBudget,
        gpu_requested: bool,
    ) -> Self {
        Self {
            scope,
            pipeline_class,
            budget,
            gpu_requested,
        }
    }

    pub fn for_procedure(
        procedure_id: ProcedureId,
        pipeline_class: PipelineClass,
        budget: ResourceBudget,
        gpu_requested: bool,
    ) -> Self {
        Self::new(
            ResourceBudgetScope::for_procedure(procedure_id),
            pipeline_class,
            budget,
            gpu_requested,
        )
    }

    pub fn for_job(
        job: impl Into<String>,
        pipeline_class: PipelineClass,
        budget: ResourceBudget,
        gpu_requested: bool,
    ) -> ResourceScopeResult<Self> {
        Ok(Self::new(
            ResourceBudgetScope::for_job(job)?,
            pipeline_class,
            budget,
            gpu_requested,
        ))
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
        let scope = self.scope.clone();

        Ok(ExecutionResourceAdmissionDecision {
            scope: scope.clone(),
            pipeline_class: self.pipeline_class,
            budget: self.budget,
            evidence: ResourceAdmissionEvidence {
                scope,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResourceAdmissionDecision {
    pub scope: ResourceBudgetScope,
    pub pipeline_class: PipelineClass,
    pub budget: ResourceBudget,
    pub evidence: ResourceAdmissionEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceAdmissionEvidence {
    pub scope: ResourceBudgetScope,
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

fn validate_non_zero_budget(
    field: ResourceLimitField,
    value: u64,
) -> Result<(), ResourceAdmissionRejection> {
    if value == 0 {
        return Err(ResourceAdmissionRejection::ZeroBudget { field });
    }
    Ok(())
}

fn validate_ram_section_budget(
    ram: &andromeda_hardware::RamProfile,
    field: ResourceLimitField,
    section: RamSectionRole,
    budget: u64,
) -> Result<(), ResourceAdmissionRejection> {
    if let Some(limit) = ram.section_budget_bytes(section)
        && budget > limit
    {
        return Err(ResourceAdmissionRejection::BudgetExceedsRamSection {
            field,
            section,
            budget,
            limit,
        });
    }
    Ok(())
}

fn validate_budget(
    budget: ResourceBudget,
    ram: &andromeda_hardware::RamProfile,
    pipeline: PipelineClass,
) -> Result<u64, ResourceAdmissionRejection> {
    validate_non_zero_budget(
        ResourceLimitField::MemoryBytes,
        budget.max_memory_bytes.bytes(),
    )?;
    validate_non_zero_budget(
        ResourceLimitField::StreamCount,
        u64::from(budget.max_streams.streams()),
    )?;
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

    validate_ram_section_budget(
        ram,
        ResourceLimitField::MemoryBytes,
        RamSectionRole::Execution,
        budget.max_memory_bytes.bytes(),
    )?;
    validate_ram_section_budget(
        ram,
        ResourceLimitField::TempBytes,
        RamSectionRole::Temp,
        budget.max_temp_bytes.bytes(),
    )?;

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
    use andromeda_types::ProcedureId;

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
        let request = ExecutionResourceAdmissionRequest::for_procedure(
            ProcedureId::new(7),
            PipelineClass::ForegroundExecution,
            ResourceBudget::new(16, 8, 2),
            false,
        );

        let decision = request.admit(&hardware(128)).unwrap();

        assert_eq!(
            decision.scope,
            ResourceBudgetScope::for_procedure(ProcedureId::new(7))
        );
        assert_eq!(decision.pipeline_class, PipelineClass::ForegroundExecution);
        assert_eq!(decision.evidence.scope, decision.scope);
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
            ExecutionResourceAdmissionRequest::for_procedure(
                ProcedureId::new(8),
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
            ExecutionResourceAdmissionRequest::for_procedure(
                ProcedureId::new(8),
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
        let reject = ExecutionResourceAdmissionRequest::for_procedure(
            ProcedureId::new(9),
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
            ExecutionResourceAdmissionRequest::for_procedure(
                ProcedureId::new(10),
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
            ExecutionResourceAdmissionRequest::for_procedure(
                ProcedureId::new(10),
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
            ExecutionResourceAdmissionRequest::for_procedure(
                ProcedureId::new(11),
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
            ExecutionResourceAdmissionRequest::for_procedure(
                ProcedureId::new(11),
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

    #[test]
    fn resource_admission_accepts_named_job_scope() {
        let request = ExecutionResourceAdmissionRequest::for_job(
            "statistics-refresh",
            PipelineClass::StatisticsRefresh,
            ResourceBudget::new(16, 8, 2),
            false,
        )
        .unwrap();

        let decision = request.admit(&hardware(128)).unwrap();

        assert_eq!(
            decision.scope,
            ResourceBudgetScope::for_job("statistics-refresh").unwrap()
        );
        assert_eq!(decision.evidence.scope, decision.scope);
        assert_eq!(decision.evidence.total_budget_bytes, 24);
    }
}
