use andromeda_core::{
    AndromedaResult, GpuExecutionPolicy, GpuProfile, HardwareProfile, PipelineClass,
};

use super::{
    constants::{COLD_ARCHIVE_RAM_BYTES, CONSERVATIVE_RAM_BYTES},
    errors::{resource_error, storage_error},
    hardware::{analytics_hardware, conservative_hardware, hot_write_hardware},
    workflow::{IoWorkflowProfile, OperationalProfileMode},
};

/// Full operational profile binding CPU, RAM, storage IO, and accelerator use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalProfile {
    pub mode: OperationalProfileMode,
    pub hardware: HardwareProfile,
    pub workflow: IoWorkflowProfile,
}

impl OperationalProfile {
    pub fn conservative() -> Self {
        Self::new(
            OperationalProfileMode::Conservative,
            conservative_hardware(CONSERVATIVE_RAM_BYTES),
            IoWorkflowProfile::conservative(),
        )
    }

    pub fn hot_write() -> Self {
        Self::new(
            OperationalProfileMode::HotWrite,
            hot_write_hardware(),
            IoWorkflowProfile::hot_write(),
        )
    }

    pub fn cold_archive() -> Self {
        Self::new(
            OperationalProfileMode::ColdArchive,
            conservative_hardware(COLD_ARCHIVE_RAM_BYTES),
            IoWorkflowProfile::cold_archive(),
        )
    }

    pub fn analytics_off_critical_path() -> Self {
        Self::new(
            OperationalProfileMode::AnalyticsOffCriticalPath,
            analytics_hardware(),
            IoWorkflowProfile::analytics_off_critical_path(),
        )
    }

    pub fn new(
        mode: OperationalProfileMode,
        hardware: HardwareProfile,
        workflow: IoWorkflowProfile,
    ) -> Self {
        Self {
            mode,
            hardware,
            workflow,
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.workflow.mode != self.mode {
            return Err(storage_error(
                "operational profile mode must match the workflow mode",
            ));
        }

        self.hardware.validate_ram_budgets()?;
        self.workflow.validate()?;
        self.validate_gpu_policy()?;

        Ok(())
    }

    pub fn validate_gpu_pipeline(&self, pipeline: PipelineClass) -> AndromedaResult<()> {
        self.hardware.validate_gpu_pipeline(pipeline)
    }

    fn validate_gpu_policy(&self) -> AndromedaResult<()> {
        for pipeline in [
            PipelineClass::Commit,
            PipelineClass::WalAppend,
            PipelineClass::Rollback,
            PipelineClass::Recovery,
        ] {
            if self
                .hardware
                .gpu
                .execution_policy
                .permits_pipeline(pipeline)
            {
                return Err(resource_error(
                    "operational profiles must not permit GPU work on critical pipelines",
                ));
            }
        }

        match self.mode {
            OperationalProfileMode::AnalyticsOffCriticalPath => {
                if !self.hardware.gpu.available
                    || self.hardware.gpu.execution_policy != GpuExecutionPolicy::BatchAnalyticsOnly
                {
                    return Err(resource_error(
                        "analytics profile requires GPU access limited to analytics pipelines",
                    ));
                }
            }
            OperationalProfileMode::Conservative
            | OperationalProfileMode::HotWrite
            | OperationalProfileMode::ColdArchive => {
                if self.hardware.gpu != GpuProfile::disabled() {
                    return Err(resource_error(
                        "non-analytics operational profiles must keep GPU execution disabled",
                    ));
                }
            }
        }

        Ok(())
    }
}

impl Default for OperationalProfile {
    fn default() -> Self {
        Self::conservative()
    }
}
